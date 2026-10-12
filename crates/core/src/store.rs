//! The single SQLite index: files, tiles, full-text, and metadata.
//!
//! Design rules from the architecture and ADRs:
//! - One SQLite file in WAL mode under the designated data dir; the index is
//!   user data and nothing here ever writes outside that directory.
//! - `meta` pins versions (schema now; models later). A schema the engine
//!   doesn't know is a loud error, never a silent mismatch.
//! - FTS5 with the trigram tokenizer gives substring matching, so `inv0ice`
//!   finds `invoice`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::index::Tile;

/// The schema version this engine understands. Bumping it means writing a
/// migration; opening a future-version store must fail loudly.
const SCHEMA_VERSION: i64 = 1;

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens (creating if needed) the index under `data_dir`.
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("creating data dir {}", data_dir.display()))?;
        let db_path: PathBuf = data_dir.join("index.sqlite");
        let conn =
            Connection::open(&db_path).with_context(|| format!("opening {}", db_path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .context("reading schema version")?;
        if version > SCHEMA_VERSION {
            anyhow::bail!(
                "index schema version {version} is newer than this build understands ({SCHEMA_VERSION}); \
                 a newer Tessera wrote this index. Refusing to touch it."
            );
        }
        if version == SCHEMA_VERSION {
            return Ok(());
        }
        self.conn
            .execute_batch(
                "BEGIN;
                 CREATE TABLE files(
                   id          INTEGER PRIMARY KEY,
                   path        TEXT UNIQUE NOT NULL,
                   kind        TEXT NOT NULL,
                   size        INTEGER NOT NULL,
                   mtime       INTEGER NOT NULL,
                   sha1        TEXT,
                   status      TEXT NOT NULL,
                   skip_reason TEXT,
                   indexed_at  INTEGER NOT NULL,
                   thumb       TEXT
                 );
                 CREATE TABLE tiles(
                   id      INTEGER PRIMARY KEY,
                   file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                   seq     INTEGER NOT NULL,
                   text    TEXT NOT NULL
                 );
                 CREATE UNIQUE INDEX tiles_file_seq ON tiles(file_id, seq);
                 CREATE VIRTUAL TABLE tiles_fts USING fts5(text, tokenize='trigram');
                 CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO meta(key, value) VALUES ('schema_version', '1');
                 COMMIT;",
            )
            .context("creating schema v1")?;
        self.conn
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .context("setting schema version")?;
        Ok(())
    }

    /// Upsert one file and replace its tiles. Idempotent: re-running on an
    /// already-indexed file leaves exactly one set of rows behind.
    pub fn index_file(&self, path: &Path, kind: &str, tiles: &[Tile]) -> Result<()> {
        let path_str = path.to_string_lossy();
        let size = path.metadata().map(|m| m.len() as i64).unwrap_or(0);
        let mtime = path
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO files(path, kind, size, mtime, sha1, status, skip_reason, indexed_at, thumb)
             VALUES (?1, ?2, ?3, ?4, NULL, 'indexed', NULL, ?5, NULL)
             ON CONFLICT(path) DO UPDATE SET
               kind = excluded.kind, size = excluded.size, mtime = excluded.mtime,
               status = 'indexed', skip_reason = NULL, indexed_at = excluded.indexed_at",
            rusqlite::params![path_str, kind, size, mtime, now],
        )?;
        let file_id: i64 = tx.query_row(
            "SELECT id FROM files WHERE path = ?1",
            rusqlite::params![path_str],
            |row| row.get(0),
        )?;
        tx.execute(
            "DELETE FROM tiles_fts WHERE rowid IN (SELECT id FROM tiles WHERE file_id = ?1)",
            rusqlite::params![file_id],
        )?;
        tx.execute(
            "DELETE FROM tiles WHERE file_id = ?1",
            rusqlite::params![file_id],
        )?;
        for tile in tiles {
            tx.execute(
                "INSERT INTO tiles(file_id, seq, text) VALUES (?1, ?2, ?3)",
                rusqlite::params![file_id, tile.seq, tile.text],
            )?;
            let tile_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO tiles_fts(rowid, text) VALUES (?1, ?2)",
                rusqlite::params![tile_id, tile.text],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Record a file that a pipeline could not process, with the reason.
    pub fn fail_file(&self, path: &Path, kind: &str, reason: &str) -> Result<()> {
        let path_str = path.to_string_lossy();
        let size = path.metadata().map(|m| m.len() as i64).unwrap_or(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO files(path, kind, size, mtime, sha1, status, skip_reason, indexed_at, thumb)
             VALUES (?1, ?2, ?3, 0, NULL, 'failed', ?4, ?5, NULL)
             ON CONFLICT(path) DO UPDATE SET
               status = 'failed', skip_reason = excluded.skip_reason,
               indexed_at = excluded.indexed_at",
            rusqlite::params![path_str, kind, size, reason, now],
        )?;
        Ok(())
    }

    /// The file ids whose tiles match one (already escaped) FTS5 term.
    pub fn file_ids_matching(&self, escaped_term: &str) -> Result<HashSet<i64>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT DISTINCT t.file_id
             FROM tiles_fts f JOIN tiles t ON t.id = f.rowid
             WHERE tiles_fts MATCH ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![escaped_term], |row| row.get::<_, i64>(0))?;
        let mut ids = HashSet::new();
        for id in rows {
            ids.insert(id?);
        }
        Ok(ids)
    }

    /// Path and kind for a file id.
    pub fn file_row(&self, file_id: i64) -> Result<(String, String)> {
        self.conn
            .query_row(
                "SELECT path, kind FROM files WHERE id = ?1",
                rusqlite::params![file_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(anyhow::Error::from)
    }

    /// All tile texts for a file, in order (used to pick a match snippet).
    pub fn tile_texts(&self, file_id: i64) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT text FROM tiles WHERE file_id = ?1 ORDER BY seq")?;
        let rows = stmt.query_map(rusqlite::params![file_id], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for text in rows {
            out.push(text?);
        }
        Ok(out)
    }

    /// Row counts by status, for `tessera stats`.
    pub fn stats(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT status, COUNT(*) FROM files GROUP BY status ORDER BY status")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Store::open(dir.path()).expect("open store");
        (dir, store)
    }

    #[test]
    fn schema_version_is_pinned() {
        let (_dir, store) = tmp_store();
        let v: String = store
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, "1");
        let uv: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(uv, SCHEMA_VERSION);
    }

    #[test]
    fn reindex_leaves_one_set_of_rows() {
        let (_dir, store) = tmp_store();
        let path = Path::new("corpus/a.txt");
        let tiles = vec![Tile {
            seq: 0,
            text: "invoice airline".into(),
        }];
        store.index_file(path, "text", &tiles).unwrap();
        store.index_file(path, "text", &tiles).unwrap();
        let files: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .unwrap();
        let tiles: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM tiles", [], |r| r.get(0))
            .unwrap();
        let fts: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM tiles_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!((files, tiles, fts), (1, 1, 1));
    }

    #[test]
    fn future_schema_is_a_loud_error() {
        let (_dir, store) = tmp_store();
        store.conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(store.migrate().is_err());
    }
}
