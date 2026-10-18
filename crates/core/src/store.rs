//! The single SQLite index: files, tiles, full-text, and metadata.
//!
//! Design rules from the architecture and ADRs:
//! - One SQLite file in WAL mode under the designated data dir; the index is
//!   user data and nothing here ever writes outside that directory.
//! - `meta` pins versions (schema now; models later). A schema the engine
//!   doesn't know is a loud error, never a silent mismatch.
//! - FTS5 runs with the trigram tokenizer over the tiles table as *external
//!   content* — the text is indexed once, not stored twice.
//! - Trigram gives substring matching (`invoic` finds `invoice`). Typo
//!   tolerance (`inv0ice` finds `invoice`) is search-side lookalike
//!   expansion, in `crate::search`.
//! - Every write that touches a file's tiles is one transaction, so a crash
//!   mid-index leaves the previous version of those tiles rather than a
//!   half-written mix.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::index::skip::{FileFacts, SkipReason};
use crate::index::{Kind, Tile};
use crate::vectors::Embedding;

/// The schema version this engine understands. Bumping it means writing a
/// migration; opening a future-version store must fail loudly.
const SCHEMA_VERSION: i64 = 4;

/// The width of a SigLIP-base embedding. Pinned by the model's own output shape,
/// so it is a constant rather than something read from a file that might be a
/// different model.
pub const IMAGE_EMBEDDING_DIM: usize = 768;

/// Schema v1, the tracer-bullet shape. Kept verbatim as the migration source
/// so a test can build a real v1 index and prove the v1 → v2 path works.
const SCHEMA_V1: &str = "BEGIN;
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
     CREATE VIRTUAL TABLE tiles_fts USING fts5(
       text, content='tiles', content_rowid='id', tokenize='trigram'
     );
     CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
     COMMIT;";

pub struct Store {
    conn: Connection,
}

/// What the index remembers about a file from a previous run — the evidence
/// change detection weighs before deciding whether to re-extract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub size: u64,
    /// What kind of file the pipeline that indexed it said it was ("image",
    /// "text", ...). Part of what the diagnostics page needs to answer about a
    /// file the user cannot find.
    pub kind: String,
    /// Nanoseconds since the Unix epoch. Nanoseconds, not seconds: two edits
    /// in the same second would otherwise be indistinguishable, and a file
    /// whose content changed without its metadata changing would stay stale
    /// forever.
    pub mtime: i64,
    /// Set only for a file this engine actually extracted. A hash we did not
    /// compute this run is not a hash we may claim.
    pub sha1: Option<String>,
    /// Whether the file was actually extracted. A skipped or failed row has no
    /// content behind it: it is work still waiting to happen, so it is never
    /// "unchanged" no matter what its metadata says.
    pub extracted: bool,
}

/// What the last index run did with a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// Extracted; its tiles are in the index.
    Indexed,
    /// Deliberately not read (see `index::skip`).
    Skipped,
    /// A pipeline could not process it.
    Failed,
    /// Read, but its content is images rather than text — a PDF with no text
    /// layer — so there is nothing to extract until an OCR pass for that
    /// format exists. Not a failure: nothing is wrong with the file, and the
    /// row stays pending work (no hash, no tiles) until the pass arrives.
    AwaitingOcr,
}

impl FileStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            FileStatus::Indexed => "indexed",
            FileStatus::Skipped => "skipped",
            FileStatus::Failed => "failed",
            FileStatus::AwaitingOcr => "awaiting_ocr",
        }
    }

    fn parse(stored: &str) -> Option<Self> {
        match stored {
            "indexed" => Some(FileStatus::Indexed),
            "skipped" => Some(FileStatus::Skipped),
            "failed" => Some(FileStatus::Failed),
            "awaiting_ocr" => Some(FileStatus::AwaitingOcr),
            _ => None,
        }
    }
}

/// One vector space: the embeddings produced by one model.
///
/// Vectors from different spaces are not comparable, so an arm is identified by
/// the model version that made them and nothing else. See the `models` module
/// for what re-embedding means in practice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm {
    /// Image↔text embeddings (SigLIP). One row per image.
    Image,
    /// Text embeddings (MiniLM). One row per text tile.
    Text,
}

impl Arm {
    /// The `meta` key recording the version that produced this arm's vectors.
    pub fn meta_key(&self) -> &'static str {
        match self {
            Arm::Image => "vectors_image_version",
            Arm::Text => "vectors_text_version",
        }
    }
}

impl std::str::FromStr for Arm {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "image" => Ok(Arm::Image),
            "text" => Ok(Arm::Text),
            other => anyhow::bail!("{other:?} is not a known arm (image | text)"),
        }
    }
}

/// A file row, as one record: the row's identity plus what the run decided
/// about it. The three outcomes differ only in `status`, `reason` and
/// `sha1`, so they share one upsert rather than three near-identical ones —
/// and the "no read, no hash" rule is stated once instead of three times.
struct FileRow<'a> {
    path: &'a Path,
    root: &'a Path,
    kind: Kind,
    facts: FileFacts,
    sha1: Option<&'a str>,
    status: FileStatus,
    reason: Option<&'a str>,
}

fn upsert_file(conn: &Connection, row: FileRow<'_>) -> Result<()> {
    conn.execute(
        "INSERT INTO files(path, root, kind, size, mtime, sha1, status, skip_reason, indexed_at, thumb)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)
         ON CONFLICT(path) DO UPDATE SET
           root = excluded.root, kind = excluded.kind, size = excluded.size,
           mtime = excluded.mtime, sha1 = excluded.sha1,
           status = excluded.status, skip_reason = excluded.skip_reason,
           indexed_at = excluded.indexed_at",
        rusqlite::params![
            row.path.to_string_lossy(),
            row.root.to_string_lossy(),
            row.kind.as_str(),
            row.facts.size as i64,
            row.facts.mtime,
            row.sha1,
            row.status.as_str(),
            row.reason,
            now_secs()
        ],
    )?;
    Ok(())
}

/// A file row as the sweeper and the move detector need it: an id and a path.
type FileRowRef = (i64, String);

/// One matched tile: its position in the file, the page it came from when the
/// format has pages, and the text itself. A result needs the tile rather than
/// a joined string because only the tile knows its page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedTile {
    pub seq: u32,
    pub page: Option<u32>,
    pub text: String,
}

/// One matched file, with everything a search result needs to describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedFile {
    pub file_id: i64,
    pub path: String,
    pub kind: String,
    /// The file's tiles in order — the text search matched against, and where
    /// a result finds out which page its snippet is on.
    pub tiles: Vec<MatchedTile>,
}

fn row_to_ref(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileRowRef> {
    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
}

impl Store {
    /// Tests and diagnostics read the stored text back through the same
    /// interface a search uses, so they cannot drift from it.
    #[cfg(test)]
    fn tile_texts(&self, file_id: i64) -> Result<Vec<String>> {
        Ok(self
            .files_by_ids(&[file_id])?
            .into_iter()
            .flat_map(|file| file.tiles)
            .map(|tile| tile.text)
            .collect())
    }

    #[cfg(test)]
    fn file_row(&self, file_id: i64) -> Result<(String, String)> {
        let file = self
            .files_by_ids(&[file_id])?
            .into_iter()
            .next()
            .context("no such file")?;
        Ok((file.path, file.kind))
    }
}

fn collect_rows(
    rows: impl Iterator<Item = rusqlite::Result<FileRowRef>>,
) -> Result<Vec<FileRowRef>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// A path the filesystem says is not there.
///
/// Only `NotFound` counts. A path that cannot be read is not evidence of
/// deletion, and treating it as one would empty an index over a permissions
/// hiccup or an unplugged drive. Private: callers ask what they actually
/// mean — "has this file moved?", "is this folder still here?" — rather than
/// deciding for themselves what absence looks like.
fn is_gone(path: &Path) -> bool {
    matches!(
        std::fs::symlink_metadata(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound
    )
}

/// `root` with the separator that must follow it inside a path, so a prefix
/// match cannot confuse a sibling folder that merely starts with the same
/// characters (`C:\corpus` must not match `C:\corpus-old\x.txt`).
fn under_prefix(root: &Path) -> String {
    let root = root.to_string_lossy();
    let mut prefix = root.trim_end_matches(std::path::MAIN_SEPARATOR).to_string();
    prefix.push(std::path::MAIN_SEPARATOR);
    prefix
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Delete a file's tiles from the tiles table and from the FTS index.
///
/// External-content FTS5 has no rows of its own to cascade: deleting a tile
/// behind its back leaves a dangling index entry, and FTS5 needs the *old*
/// text supplied to delete one. So the text is read out first, then the
/// mirrored rows go.
fn purge_tiles(tx: &Transaction<'_>, file_id: i64) -> Result<()> {
    let old: Vec<(i64, String)> = {
        let mut stmt = tx.prepare("SELECT id, text FROM tiles WHERE file_id = ?1")?;
        let rows = stmt.query_map(rusqlite::params![file_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    for (tile_id, text) in &old {
        tx.execute(
            "INSERT INTO tiles_fts(tiles_fts, rowid, text) VALUES ('delete', ?1, ?2)",
            rusqlite::params![tile_id, text],
        )?;
    }
    tx.execute(
        "DELETE FROM tiles WHERE file_id = ?1",
        rusqlite::params![file_id],
    )?;
    Ok(())
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
        let mut version = version;
        while version < SCHEMA_VERSION {
            version += 1;
            self.apply_migration(version)?;
        }
        Ok(())
    }

    /// One version step. Each step ends with `user_version` and the `meta`
    /// mirror written from the same value, so the two can never drift apart.
    fn apply_migration(&self, version: i64) -> Result<()> {
        match version {
            1 => self
                .conn
                .execute_batch(SCHEMA_V1)
                .context("creating schema v1")?,
            2 => self
                .conn
                .execute_batch(
                    "BEGIN;
                     ALTER TABLE files ADD COLUMN root TEXT NOT NULL DEFAULT '';
                     CREATE INDEX files_root_sha1 ON files(root, sha1);
                     COMMIT;",
                )
                .context("migrating schema to v2")?,
            3 => self
                .conn
                .execute_batch(
                    "BEGIN;
                     CREATE TABLE IF NOT EXISTS img_vec(
                       file_id INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
                       dim     INTEGER NOT NULL,
                       vec     BLOB NOT NULL
                     );
                     COMMIT;",
                )
                .context("migrating schema to v3")?,
            4 => self
                .conn
                .execute_batch(
                    "BEGIN;
                     ALTER TABLE tiles ADD COLUMN page INTEGER;
                     COMMIT;",
                )
                .context("migrating schema to v4")?,
            other => anyhow::bail!("no migration for schema version {other}"),
        }
        self.conn
            .pragma_update(None, "user_version", version)
            .context("setting schema version")?;
        self.conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', ?1)",
            rusqlite::params![version.to_string()],
        )?;
        Ok(())
    }

    /// What the index knows about `path`, or `None` if it has never seen it.
    pub fn file_state(&self, path: &Path) -> Result<Option<FileState>> {
        self.conn
            .query_row(
                "SELECT size, mtime, sha1, status, kind FROM files WHERE path = ?1",
                rusqlite::params![path.to_string_lossy()],
                |row| {
                    let status: String = row.get(3)?;
                    Ok(FileState {
                        size: row.get::<_, i64>(0)? as u64,
                        mtime: row.get(1)?,
                        sha1: row.get(2)?,
                        extracted: FileStatus::parse(&status) == Some(FileStatus::Indexed),
                        kind: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(anyhow::Error::from)
    }

    /// Every file that failed to index, with the reason recorded at the time.
    ///
    /// "Search can't find my scanned invoice" is not answerable from a count.
    /// This is the other half of that answer: which files, and what the engine
    /// said about each one.
    pub fn failures(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT path, COALESCE(skip_reason, '') FROM files
             WHERE status = 'failed' ORDER BY path",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Rows belonging to one indexed folder, with their paths. The deletion
    /// sweep works from this list, scoped to the folder that was just walked.
    ///
    /// Rows written before roots existed (`root = ''`) are matched by path, so
    /// a corpus indexed by an older build still gets its deletions.
    fn rows_under(&self, root: &Path) -> Result<Vec<FileRowRef>> {
        let prefix = under_prefix(root);
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, path FROM files
             WHERE root = ?1 OR (root = '' AND (path = ?2 OR substr(path, 1, length(?2)) = ?2))",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![root.to_string_lossy(), prefix],
            row_to_ref,
        )?;
        collect_rows(rows)
    }

    /// Rows under `root` whose content hash is `sha1`, excluding `keep`. The
    /// raw material behind [`Store::moved_candidate`].
    fn rows_matching_sha1(&self, root: &Path, sha1: &str, keep: &Path) -> Result<Vec<FileRowRef>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, path FROM files WHERE root = ?1 AND sha1 = ?2 AND path != ?3",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![root.to_string_lossy(), sha1, keep.to_string_lossy()],
            row_to_ref,
        )?;
        collect_rows(rows)
    }

    /// The row this file's content belongs to, if it used to live elsewhere.
    ///
    /// A move is one fact — "these bytes were that row" — so it is asked in
    /// one call: the content hash, the folder it came from, and the check that
    /// the old path is really gone. Two live copies of the same bytes are two
    /// files, and a hash match is not evidence that anything moved.
    ///
    /// The hash is only ever set on a row this engine extracted, so a row that
    /// was skipped or failed cannot be dragged to a new path by a stale hash.
    pub fn moved_candidate(
        &self,
        root: &Path,
        sha1: &str,
        keep: &Path,
    ) -> Result<Option<FileRowRef>> {
        Ok(self
            .rows_matching_sha1(root, sha1, keep)?
            .into_iter()
            .find(|(_, old)| is_gone(Path::new(old))))
    }

    /// Drop rows for files under `root` that are no longer on disk, so a
    /// deletion propagates instead of leaving behind a result nobody can open.
    ///
    /// Scoped to the folder that was just walked, so indexing one folder never
    /// forgets another. Existence is checked directly rather than inferred
    /// from what the walk returned, because a directory the user cannot read
    /// looks exactly like an empty one.
    pub fn remove_missing(&self, root: &Path) -> Result<usize> {
        let mut removed = 0;
        for (id, path) in self.rows_under(root)? {
            if is_gone(Path::new(&path)) {
                self.remove_row(id)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Upsert one file and replace its tiles, in one transaction. Idempotent:
    /// re-running on an already-indexed file leaves exactly one set of rows.
    pub fn index_file(
        &self,
        root: &Path,
        path: &Path,
        kind: Kind,
        facts: &FileFacts,
        sha1: Option<&str>,
        tiles: &[Tile],
    ) -> Result<()> {
        let path_str = path.to_string_lossy();
        let tx = self.conn.unchecked_transaction()?;
        upsert_file(
            &tx,
            FileRow {
                path,
                root,
                kind,
                facts: *facts,
                sha1,
                status: FileStatus::Indexed,
                reason: None,
            },
        )?;
        let file_id: i64 = tx.query_row(
            "SELECT id FROM files WHERE path = ?1",
            rusqlite::params![path_str],
            |row| row.get(0),
        )?;

        purge_tiles(&tx, file_id)?;
        for tile in tiles {
            tx.execute(
                "INSERT INTO tiles(file_id, seq, page, text) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![file_id, tile.seq, tile.page, tile.text],
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

    /// Record a file that was deliberately not read, with the reason. The
    /// content hash is cleared: nothing was read this run, so keeping an old
    /// hash would claim evidence this run does not have.
    pub fn skip_file(
        &self,
        root: &Path,
        path: &Path,
        kind: Kind,
        facts: &FileFacts,
        reason: SkipReason,
    ) -> Result<()> {
        // Tiles from a previous run are left alone: a file that was readable
        // and is now a cloud placeholder is still *known*, and searching the
        // text we already hold downloads nothing. Losing it would make search
        // results vanish for a reason the user did not cause and cannot undo.
        upsert_file(
            &self.conn,
            FileRow {
                path,
                root,
                kind,
                facts: *facts,
                sha1: None,
                status: FileStatus::Skipped,
                reason: Some(reason.as_str()),
            },
        )
    }

    /// Record a file that a pipeline could not process, with the reason. The
    /// metadata is the file's real metadata, and its hash is dropped: nothing
    /// was extracted, so there is no content to vouch for.
    pub fn fail_file(
        &self,
        root: &Path,
        path: &Path,
        kind: Kind,
        facts: &FileFacts,
        reason: &str,
    ) -> Result<()> {
        upsert_file(
            &self.conn,
            FileRow {
                path,
                root,
                kind,
                facts: *facts,
                sha1: None,
                status: FileStatus::Failed,
                reason: Some(reason),
            },
        )
    }

    /// Record a file that was read but has nothing this build can read out of
    /// it — a PDF with no text layer, waiting for an OCR pass. Not `failed`
    /// (nothing is wrong with the file) and not `skipped` (which is for files
    /// deliberately not read at all).
    ///
    /// The hash is dropped like a skip's and a failure's: no content was
    /// extracted, so there is nothing to vouch for, and the row is pending
    /// work — the next index run re-reads it, and the OCR pass that arrives
    /// later finds it by its status. No tiles are written, both because there
    /// are none and because a name tile would claim the file is searchable
    /// when its content is exactly what is missing.
    pub fn await_ocr(&self, root: &Path, path: &Path, kind: Kind, facts: &FileFacts) -> Result<()> {
        upsert_file(
            &self.conn,
            FileRow {
                path,
                root,
                kind,
                facts: *facts,
                sha1: None,
                status: FileStatus::AwaitingOcr,
                reason: Some("no_text_layer"),
            },
        )
    }

    /// Refresh the metadata of a file whose content is already indexed, and
    /// clear any skip reason: whatever kept it out of the index is gone.
    pub fn mark_unchanged(&self, path: &Path, facts: &FileFacts) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET size = ?2, mtime = ?3, status = ?4, skip_reason = NULL, indexed_at = ?5
             WHERE path = ?1",
            rusqlite::params![
                path.to_string_lossy(),
                facts.size as i64,
                facts.mtime,
                FileStatus::Indexed.as_str(),
                now_secs()
            ],
        )?;
        Ok(())
    }

    /// Move a file row to a new path: the same bytes, a new name.
    ///
    /// The filename lives at the head of the file's first tile (see
    /// `index::with_filename`), so a move has to re-title it — a moved file
    /// must be findable under the name it has *now*, without re-extracting
    /// anything. That convention is enforced here, inside the transaction that
    /// updates the row, rather than asked of the caller: one transaction, so
    /// the path and the search text can never disagree.
    pub fn move_file(
        &self,
        file_id: i64,
        new_path: &Path,
        facts: &FileFacts,
        sha1: &str,
    ) -> Result<()> {
        let new_name = new_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let tx = self.conn.unchecked_transaction()?;
        let first_tile: Option<String> = tx
            .query_row(
                "SELECT text FROM tiles WHERE file_id = ?1 AND seq = 0",
                rusqlite::params![file_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(old_title) = first_tile {
            let title = crate::index::retitled(&old_title, new_name);
            let tile_id: i64 = tx.query_row(
                "SELECT id FROM tiles WHERE file_id = ?1 AND seq = 0",
                rusqlite::params![file_id],
                |row| row.get(0),
            )?;
            tx.execute(
                "INSERT INTO tiles_fts(tiles_fts, rowid, text) VALUES ('delete', ?1, ?2)",
                rusqlite::params![tile_id, old_title],
            )?;
            tx.execute(
                "UPDATE tiles SET text = ?2 WHERE id = ?1",
                rusqlite::params![tile_id, title],
            )?;
            tx.execute(
                "INSERT INTO tiles_fts(rowid, text) VALUES (?1, ?2)",
                rusqlite::params![tile_id, title],
            )?;
        }
        tx.execute(
            "UPDATE files SET
               path = ?2, size = ?3, mtime = ?4, sha1 = ?5,
               status = 'indexed', skip_reason = NULL
             WHERE id = ?1",
            rusqlite::params![
                file_id,
                new_path.to_string_lossy(),
                facts.size as i64,
                facts.mtime,
                sha1
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Forget a file: its tiles leave the full-text index and the store. Used
    /// when the file is gone from disk, so nothing silently lingers.
    fn remove_row(&self, file_id: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        purge_tiles(&tx, file_id)?;
        tx.execute(
            "DELETE FROM files WHERE id = ?1",
            rusqlite::params![file_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The file ids whose tiles match one FTS5 MATCH expression
    /// (see `crate::search::term_match_expr`).
    pub fn file_ids_matching(&self, match_expr: &str) -> Result<HashSet<i64>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT DISTINCT t.file_id
             FROM tiles_fts f JOIN tiles t ON t.id = f.rowid
             WHERE tiles_fts MATCH ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![match_expr], |row| row.get::<_, i64>(0))?;
        let mut ids = HashSet::new();
        for id in rows {
            ids.insert(id?);
        }
        Ok(ids)
    }

    /// These files, whole: identity, kind, and searchable text, in one round
    /// trip.
    ///
    /// Asking per id — one query for the row, another for its tiles — costs
    /// two round trips per hit, and fusion multiplies that by the number of
    /// arms. Callers hold the id intersection they already computed; this
    /// resolves it once. Snippet building stays in `crate::search`: choosing a
    /// window around a match is a decision about presentation, not storage.
    ///
    /// Ids are the ones `file_ids_matching` returned, so they are already in
    /// ascending order from the intersection; rows come back grouped per file.
    pub fn files_by_ids(&self, file_ids: &[i64]) -> Result<Vec<MatchedFile>> {
        if file_ids.is_empty() {
            return Ok(Vec::new());
        }
        // One bind parameter per id, filled from the slice.
        let holes = (1..=file_ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT f.id, f.path, f.kind, t.seq, t.page, t.text
             FROM files f JOIN tiles t ON t.file_id = f.id
             WHERE f.id IN ({holes})
             ORDER BY f.id, t.seq"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(file_ids), |row| {
            Ok(MatchedFile {
                file_id: row.get::<_, i64>(0)?,
                path: row.get(1)?,
                kind: row.get::<_, String>(2)?,
                tiles: vec![MatchedTile {
                    seq: row.get::<_, u32>(3)?,
                    page: row.get::<_, Option<u32>>(4)?,
                    text: row.get(5)?,
                }],
            })
        })?;
        let mut out: Vec<MatchedFile> = Vec::new();
        for row in rows {
            let mut file = row?;
            // Consecutive rows of one file collapse into one result.
            match out.last_mut() {
                Some(last) if last.file_id == file.file_id => {
                    last.tiles.append(&mut file.tiles);
                }
                _ => out.push(file),
            }
        }
        Ok(out)
    }

    /// The model version whose vectors `arm` currently holds, or `None` if
    /// that arm has never been embedded.
    ///
    /// Recorded beside the vectors, not beside the model: the question this
    /// answers is "are these numbers comparable with each other?", which is
    /// about the vectors, not about what is on disk today.
    pub fn arm_version(&self, arm: Arm) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                rusqlite::params![arm.meta_key()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(anyhow::Error::from)
    }

    /// Record that `arm`'s vectors were built by `version`.
    pub fn set_arm_version(&self, arm: Arm, version: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
            rusqlite::params![arm.meta_key(), version],
        )?;
        Ok(())
    }

    /// Forget that `arm` has vectors. Used when a model pin moves: the vectors
    /// are about to be dropped by the arm that owns them, and until then the
    /// index must report the arm as needing work rather than searching with
    /// numbers from two different models.
    pub fn clear_arm(&self, arm: Arm) -> Result<()> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            rusqlite::params![arm.meta_key()],
        )?;
        Ok(())
    }

    /// Store an image's embedding, normalized here rather than by the caller.
    ///
    /// Normalizing at the boundary is what lets [`crate::vectors::cosine`] be a
    /// plain dot product, and it means a row written by any future path is
    /// comparable with a row written by this one. The vector is stored against
    /// the file's id, so re-indexing a file replaces its vector rather than
    /// accumulating them.
    pub fn put_image_embedding(&self, file_id: i64, embedding: &Embedding) -> Result<()> {
        let mut stored = embedding.clone();
        stored.normalize();
        self.conn.execute(
            "INSERT OR REPLACE INTO img_vec(file_id, dim, vec) VALUES (?1, ?2, ?3)",
            rusqlite::params![file_id, stored.dim() as i64, stored.to_blob()],
        )?;
        Ok(())
    }

    /// The id of the file at `path`, for a caller that holds a path and needs
    /// to attach a vector to it.
    pub fn file_id(&self, path: &Path) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT id FROM files WHERE path = ?1",
                rusqlite::params![path.to_string_lossy()],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    /// How many image files currently carry a vector.
    ///
    /// This is the number that answers "did re-indexing actually fill the arm
    /// in?", which is otherwise invisible: a missing vector does not fail a
    /// query, it just quietly returns fewer results.
    pub fn image_embedding_count(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM img_vec", [], |row| row.get(0))
            .map_err(Into::into)
    }

    /// Every stored image vector, paired with its file id.
    ///
    /// The scan takes the whole arm because the ranking is over all of it.
    ///
    /// A row that will not decode is **quarantined, not fatal**: one corrupt
    /// row must not take description search down for every other image. The
    /// file is named on stderr so it can be re-indexed and the scan carries on
    /// without it — the same rule the indexer follows for a file it cannot read.
    pub fn image_embeddings(&self, dim: usize) -> Result<Vec<(i64, Embedding)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT file_id, vec FROM img_vec ORDER BY file_id")?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let file_id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            match Embedding::from_blob(&blob, dim) {
                Ok(embedding) => out.push((file_id, embedding)),
                Err(err) => eprintln!(
                    "skipping a corrupt vector for image file {file_id}: {err:#}. Re-index that \
                     file to repair it; every other image is still searchable by description."
                ),
            }
        }
        Ok(out)
    }

    /// The images that best match `query`, best first.
    ///
    /// This is the description arm on its own — cosine against every stored vector.
    /// It is deliberately **not** fused with the lexical arm, because measurement
    /// says fusing them on text-bearing files makes results worse: on 56 real
    /// screenshots, text alone retrieves 0.964, the image arm alone 0.268, and
    /// naive Reciprocal Rank Fusion over both scores 0.375. Fusion has to be
    /// weighted by whether a file has text at all, which is ticket 10 and is not
    /// what a one-box keyword search does.
    ///
    /// Only files that are *images* are eligible, so a document chunk never wins a
    /// description query by accident.
    pub fn describe(
        &self,
        query: &Embedding,
        dim: usize,
        limit: usize,
    ) -> Result<Vec<MatchedFile>> {
        let embeddings = self.image_embeddings(dim)?;
        let ranked = crate::vectors::rank(
            query,
            embeddings.iter().map(|(id, e)| (*id, e)).collect(),
            limit,
        );
        let ids: Vec<i64> = ranked.iter().map(|r| r.file_id).collect();
        // A dedicated query rather than `files_by_ids`, which inner-joins
        // `tiles`. A photo of a dog has no tiles at all — there is nothing to
        // read from it — so that join drops it, and description search would be
        // unable to return precisely the textless images it exists to find. The
        // text is fetched alongside for the preview line and is allowed to be
        // absent.
        let holes = (1..=ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT f.id, f.path, f.kind, t.seq, t.page, t.text
             FROM files f
             LEFT JOIN tiles t ON t.id =
                 (SELECT t2.id FROM tiles t2 WHERE t2.file_id = f.id ORDER BY t2.seq LIMIT 1)
             WHERE f.id IN ({holes})"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(&ids), |row| {
            let seq: Option<u32> = row.get(3)?;
            let page: Option<u32> = row.get(4)?;
            let text: Option<String> = row.get(5)?;
            let tiles = match (seq, text) {
                (Some(seq), Some(text)) => vec![MatchedTile { seq, page, text }],
                _ => Vec::new(),
            };
            Ok(MatchedFile {
                file_id: row.get::<_, i64>(0)?,
                path: row.get(1)?,
                kind: row.get::<_, String>(2)?,
                tiles,
            })
        })?;
        let mut by_id: std::collections::HashMap<i64, MatchedFile> =
            std::collections::HashMap::new();
        for row in rows {
            let file: MatchedFile = row?;
            by_id.insert(file.file_id, file);
        }
        // Back into score order, which is the entire point of the query.
        Ok(ranked
            .iter()
            .filter_map(|r| by_id.get(&r.file_id).cloned())
            .collect())
    }

    /// Forget the image arm: drop every vector **and** clear its version pin, in
    /// one transaction.
    ///
    /// The order and the atomicity both matter, and this exists rather than two
    /// separate calls because getting them wrong is silent. If the version were
    /// cleared first and the process died before the vectors went, the index
    /// would report the arm as outstanding work — fine. If the vectors went
    /// first and the version survived, the index would report the arm as
    /// **complete** while every description query silently returned nothing,
    /// which is precisely the failure the version pin exists to prevent.
    pub fn forget_image_arm(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM img_vec", [])?;
        tx.execute(
            "DELETE FROM meta WHERE key = ?1",
            rusqlite::params![Arm::Image.meta_key()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Forget every image vector, leaving the version pin alone.
    ///
    /// Prefer [`Store::forget_image_arm`]: this leaves the index in the
    /// "arm complete, vectors gone" state described there.
    ///
    /// The vectors in `img_vec` were produced by a model that is no longer the
    /// one this
    /// build uses, and a vector from one model is meaningless against a query
    /// from another.
    pub fn drop_image_embeddings(&self) -> Result<usize> {
        self.conn
            .execute("DELETE FROM img_vec", [])
            .map_err(Into::into)
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

    /// Skipped files grouped by reason, for the `tessera stats` breakdown.
    /// Failed files carry a reason too, but theirs is an error message: it
    /// does not belong in a breakdown of *why files were not read*.
    pub fn skip_reasons(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT skip_reason, COUNT(*) FROM files
             WHERE status = 'skipped' AND skip_reason IS NOT NULL
             GROUP BY skip_reason ORDER BY skip_reason",
        )?;
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
    use crate::index::skip::FileFacts;
    use crate::vectors::cosine;

    fn tmp_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Store::open(dir.path()).expect("open store");
        (dir, store)
    }

    /// A four-wide stand-in for a SigLIP vector, so the storage tests do not
    /// carry 768 floats to say "a vector was written".
    const DIM: usize = 4;

    fn embedding(values: [f32; DIM]) -> Embedding {
        Embedding::new(values.to_vec(), DIM).expect("the right width")
    }

    fn indexed_image(store: &Store, dir: &Path, name: &str) -> i64 {
        let path = dir.join(name);
        std::fs::write(&path, b"not really a png").unwrap();
        store
            .index_file(
                dir,
                &path,
                Kind::Image,
                &facts(10),
                Some("abc"),
                &[Tile {
                    seq: 0,
                    text: "a screenshot".into(),
                    page: None,
                }],
            )
            .expect("index it");
        store.file_id(&path).expect("and find its id")
    }

    #[test]
    fn a_stored_vector_comes_back_as_the_same_direction() {
        let (_dir, store) = tmp_store();
        let corpus = tempfile::tempdir().unwrap();
        let file_id = indexed_image(&store, corpus.path(), "a.png");

        // Deliberately not unit length: the store normalizes on write, so what
        // comes back must still point the same way.
        store
            .put_image_embedding(file_id, &embedding([3.0, 4.0, 0.0, 0.0]))
            .expect("write it");

        assert_eq!(store.image_embedding_count().unwrap(), 1);
        let stored = store.image_embeddings(DIM).expect("read it back");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].0, file_id);
        let norm = stored[0]
            .1
            .values()
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            .sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-6,
            "the store must normalize on write, got a norm of {norm}"
        );
        assert!(
            (cosine(&stored[0].1, &embedding([3.0, 4.0, 0.0, 0.0]).normalized()) - 1.0).abs()
                < 1e-6
        );
    }

    #[test]
    fn re_indexing_a_file_replaces_its_vector_rather_than_adding_one() {
        let (_dir, store) = tmp_store();
        let corpus = tempfile::tempdir().unwrap();
        let file_id = indexed_image(&store, corpus.path(), "a.png");

        store
            .put_image_embedding(file_id, &embedding([1.0, 0.0, 0.0, 0.0]))
            .unwrap();
        store
            .put_image_embedding(file_id, &embedding([0.0, 1.0, 0.0, 0.0]))
            .unwrap();

        assert_eq!(
            store.image_embedding_count().unwrap(),
            1,
            "re-embedding must replace, not accumulate"
        );
        let stored = store.image_embeddings(DIM).unwrap();
        assert!(
            cosine(&stored[0].1, &embedding([0.0, 1.0, 0.0, 0.0]).normalized()) > 0.99,
            "the second vector should have won"
        );
    }

    #[test]
    fn deleting_a_file_takes_its_vector_with_it() {
        let (_dir, store) = tmp_store();
        let corpus = tempfile::tempdir().unwrap();
        let file_id = indexed_image(&store, corpus.path(), "a.png");
        store
            .put_image_embedding(file_id, &embedding([1.0, 0.0, 0.0, 0.0]))
            .unwrap();

        std::fs::remove_file(corpus.path().join("a.png")).unwrap();
        store.remove_missing(corpus.path()).expect("sweep");

        assert_eq!(
            store.image_embedding_count().unwrap(),
            0,
            "a vector outliving its file would be returned for a path that is gone"
        );
    }

    #[test]
    fn one_corrupt_vector_does_not_take_the_whole_arm_down() {
        // A single bad row must not make description search fail for every
        // other image. The corrupt row is dropped and named on stderr; the good
        // one still comes back. Failing the whole scan would mean one
        // re-index interrupted mid-write could cost the user the entire arm.
        let (_dir, store) = tmp_store();
        let corpus = tempfile::tempdir().unwrap();
        let good = indexed_image(&store, corpus.path(), "good.png");
        let corrupt = indexed_image(&store, corpus.path(), "corrupt.png");
        store
            .put_image_embedding(good, &embedding([1.0, 0.0, 0.0, 0.0]))
            .unwrap();
        // 8 bytes: not a whole number of 4-byte floats.
        store
            .conn
            .execute(
                "INSERT INTO img_vec(file_id, dim, vec) VALUES (?1, ?2, ?3)",
                rusqlite::params![corrupt, DIM as i64, vec![0u8; 8]],
            )
            .unwrap();

        let read = store
            .image_embeddings(DIM)
            .expect("the scan must survive one bad row");
        assert_eq!(
            read.len(),
            1,
            "the corrupt row should be dropped and the good one kept"
        );
        assert_eq!(read[0].0, good, "the wrong image survived");
        // The row is still in the table, which is what the count reports: the
        // point is that it costs one file, not the arm.
        assert_eq!(store.image_embedding_count().unwrap(), 2);
    }

    #[test]
    fn a_vector_of_the_wrong_width_is_still_caught_by_the_decoder() {
        // The decoder itself refuses a truncated blob. The scan quarantines it;
        // this pins that the refusal is the decoder's and not the scan's.
        let err = Embedding::from_blob(&[0u8; 8], DIM).expect_err("8 bytes is not 4 floats");
        assert!(format!("{err:#}").contains("not 4 float32"), "{err:#}");
    }

    #[test]
    fn a_changed_pin_makes_the_image_arm_outstanding_work_and_forgets_its_vectors() {
        // Ticket 07's acceptance criterion, and the reason `meta` pins model
        // versions at all: a vector from one model is meaningless against a
        // query from another, so a moved pin must invalidate the arm rather
        // than blend the two.
        let (_dir, store) = tmp_store();
        let corpus = tempfile::tempdir().unwrap();
        let file_id = indexed_image(&store, corpus.path(), "a.png");

        store
            .set_arm_version(Arm::Image, "pin-old")
            .expect("record the pin this arm was built with");
        store
            .put_image_embedding(file_id, &embedding([1.0, 0.0, 0.0, 0.0]))
            .unwrap();
        assert_eq!(
            store.arm_version(Arm::Image).unwrap().as_deref(),
            Some("pin-old")
        );

        // The pin moves: forget the arm — vectors and version, together.
        store.forget_image_arm().expect("forget the arm");

        assert_eq!(
            store.arm_version(Arm::Image).unwrap(),
            None,
            "a forgotten arm has no version, so it reads as outstanding work"
        );
        assert_eq!(
            store.image_embedding_count().unwrap(),
            0,
            "vectors from a model this build no longer uses must not survive"
        );
        assert_eq!(
            store.arm_version(Arm::Text).unwrap(),
            None,
            "clearing one arm must not touch another"
        );
    }

    fn facts(size: u64) -> FileFacts {
        FileFacts {
            size,
            mtime: 1_000,
            placeholder: false,
        }
    }

    fn tiles(texts: &[&str]) -> Vec<Tile> {
        texts
            .iter()
            .enumerate()
            .map(|(seq, text)| Tile {
                seq: seq as u32,
                text: (*text).to_string(),
                page: None,
            })
            .collect()
    }

    fn indexed_file(store: &Store, path: &str, texts: &[&str]) {
        store
            .index_file(
                Path::new("/corpus"),
                Path::new(path),
                Kind::Text,
                &facts(10),
                Some("abc123"),
                &tiles(texts),
            )
            .unwrap();
    }

    #[test]
    fn the_index_is_a_wal_database_under_the_data_dir() {
        // SECURITY.md names "writes outside the index and cache directories"
        // as a bug by definition. WAL mode is the half of that which was
        // configured but never asserted: deleting `pragma_update(... "WAL")`
        // would have left the whole suite green while the index silently ran in
        // rollback-journal mode.
        let (_dir, store) = tmp_store();
        let mode: String = store
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            mode.to_lowercase(),
            "wal",
            "the index must be a WAL database"
        );

        // And the database file is inside the data dir we were handed, by the
        // name the engine opens. Nothing else is written there.
        let entries: Vec<String> = std::fs::read_dir(_dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            entries.contains(&"index.sqlite".to_string()),
            "the index belongs at <data dir>/index.sqlite; found {entries:?}"
        );
        // WAL means the connection may also hold -wal/-shm sidecars; anything
        // else appearing in the data dir is a file the engine did not mean to
        // create.
        for name in &entries {
            assert!(
                matches!(
                    name.as_str(),
                    "index.sqlite" | "index.sqlite-wal" | "index.sqlite-shm"
                ),
                "unexpected file in the data dir: {name}"
            );
        }
    }

    #[test]
    fn schema_version_is_pinned_once_in_both_places() {
        let (_dir, store) = tmp_store();
        let meta: String = store
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let uv: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(meta.parse::<i64>().unwrap(), uv);
        assert_eq!(uv, SCHEMA_VERSION);
    }

    #[test]
    fn a_v1_index_migrates_without_losing_tiles() {
        let dir = tempfile::tempdir().unwrap();
        // Native paths, because that is what the engine stores and what the
        // sweep's prefix match has to agree with.
        let corpus = PathBuf::from("corpus");
        let file = corpus.join("a.txt");
        {
            let conn = Connection::open(dir.path().join("index.sqlite")).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute(
                "INSERT INTO files(path, kind, size, mtime, sha1, status, skip_reason, indexed_at, thumb)
                 VALUES (?1, 'text', 10, 20, 'abc123', 'indexed', NULL, 30, NULL)",
                rusqlite::params![file.to_string_lossy()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO tiles(file_id, seq, text) VALUES (1, 0, 'invoice')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO tiles_fts(rowid, text) VALUES (1, 'invoice')",
                [],
            )
            .unwrap();
        }

        let store = Store::open(dir.path()).unwrap();
        // the row survives, and so does its searchable text
        let state = store.file_state(&file).unwrap().unwrap();
        assert_eq!(state.size, 10);
        assert_eq!(state.sha1.as_deref(), Some("abc123"));
        assert!(state.extracted);
        assert_eq!(store.tile_texts(1).unwrap(), vec!["invoice"]);
        assert_eq!(
            store
                .file_ids_matching(&crate::search::term_match_expr("invoice"))
                .unwrap()
                .len(),
            1
        );
        // rows written before roots existed belong to no root, but the sweep
        // still finds them by path — or every deletion from before the upgrade
        // would linger forever
        assert_eq!(store.rows_under(Path::new("")).unwrap().len(), 1);
        assert_eq!(
            store.rows_under(&corpus).unwrap(),
            vec![(1, file.to_string_lossy().to_string())]
        );
        assert!(
            store
                .rows_under(Path::new("corpus-old"))
                .unwrap()
                .is_empty(),
            "a sibling folder sharing a name prefix must not match"
        );
    }

    #[test]
    fn reindex_leaves_one_set_of_rows() {
        let (_dir, store) = tmp_store();
        indexed_file(&store, "/corpus/a.txt", &["invoice airline"]);
        indexed_file(&store, "/corpus/a.txt", &["invoice airline"]);
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
    fn reindex_with_new_text_replaces_the_old() {
        let (_dir, store) = tmp_store();
        indexed_file(&store, "/corpus/a.txt", &["old words"]);
        indexed_file(&store, "/corpus/a.txt", &["brand new words"]);
        let ids = store
            .file_ids_matching(&crate::search::term_match_expr("brand"))
            .unwrap();
        let gone = store
            .file_ids_matching(&crate::search::term_match_expr("old"))
            .unwrap();
        assert_eq!(ids.len(), 1);
        assert!(gone.is_empty());
    }

    #[test]
    fn a_tiles_page_survives_the_round_trip() {
        let (_dir, store) = tmp_store();
        store
            .index_file(
                Path::new("/corpus"),
                Path::new("/corpus/report.pdf"),
                Kind::Pdf,
                &facts(10),
                Some("abc123"),
                &[
                    Tile {
                        seq: 0,
                        text: "a report".into(),
                        page: Some(1),
                    },
                    Tile {
                        seq: 1,
                        text: "more text".into(),
                        page: Some(2),
                    },
                    Tile {
                        seq: 2,
                        text: "no page here".into(),
                        page: None,
                    },
                ],
            )
            .unwrap();
        let mut files = store.files_by_ids(&[1]).unwrap();
        let file = files.remove(0);
        let pages: Vec<Option<u32>> = file.tiles.iter().map(|t| t.page).collect();
        assert_eq!(
            pages,
            vec![Some(1), Some(2), None],
            "pages are what the pipeline said, not positions"
        );
        let texts: Vec<&str> = file.tiles.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, vec!["a report", "more text", "no page here"]);
    }

    #[test]
    fn an_awaiting_file_is_recorded_without_claiming_anything() {
        let (_dir, store) = tmp_store();
        store
            .await_ocr(
                Path::new("/corpus"),
                Path::new("/corpus/scan.pdf"),
                Kind::Pdf,
                &facts(2048),
            )
            .unwrap();

        let state = store
            .file_state(Path::new("/corpus/scan.pdf"))
            .unwrap()
            .expect("the file is recorded");
        assert!(!state.extracted, "the file is pending work, not indexed");
        assert_eq!(
            state.sha1, None,
            "nothing was extracted, so nothing is claimed"
        );
        assert_eq!(
            store.stats().unwrap(),
            vec![("awaiting_ocr".to_string(), 1)]
        );
        assert!(
            store.skip_reasons().unwrap().is_empty(),
            "awaiting OCR is not a pre-read skip and does not belong in that breakdown"
        );
        assert!(
            store
                .moved_candidate(
                    Path::new("/corpus"),
                    "abc123",
                    Path::new("/corpus/moved.pdf")
                )
                .unwrap()
                .is_none(),
            "an awaiting row carries no hash and cannot be moved by one"
        );
        assert!(
            store.files_by_ids(&[1]).unwrap().is_empty(),
            "no tiles means the file is not searchable by its content"
        );
    }

    /// The transactional promise: a rejected upsert must leave the previous
    /// version of the tiles — in the tiles table *and* in the FTS index —
    /// exactly as it was, not a mixture of old and new.
    #[test]
    fn a_rejected_upsert_rolls_back_completely() {
        let (_dir, store) = tmp_store();
        indexed_file(&store, "/corpus/a.txt", &["invoice airline"]);
        // two tiles claiming the same position: the (file_id, seq) index
        // refuses the second one, mid-transaction
        let clashing = vec![
            Tile {
                seq: 0,
                text: "brand new words".into(),
                page: None,
            },
            Tile {
                seq: 0,
                text: "second tile".into(),
                page: None,
            },
        ];
        let err = store.index_file(
            Path::new("/corpus"),
            Path::new("/corpus/a.txt"),
            Kind::Text,
            &facts(11),
            Some("def456"),
            &clashing,
        );
        assert!(err.is_err(), "duplicate seq must be rejected");
        assert_eq!(store.tile_texts(1).unwrap(), vec!["invoice airline"]);
        assert_eq!(
            store
                .file_ids_matching(&crate::search::term_match_expr("invoice"))
                .unwrap()
                .len(),
            1,
            "the old text must still be searchable: the fts deletes rolled back too"
        );
        assert!(store
            .file_ids_matching(&crate::search::term_match_expr("brand"))
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .file_state(Path::new("/corpus/a.txt"))
                .unwrap()
                .unwrap()
                .sha1,
            Some("abc123".to_string()),
            "the rolled-back upsert must not have recorded its hash"
        );
    }

    #[test]
    fn fts_is_external_content_no_duplicated_text() {
        let (_dir, store) = tmp_store();
        indexed_file(&store, "/corpus/a.txt", &["hello world"]);
        // The fts table stores no text of its own; tiles stores it once.
        let stored: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tiles_fts_data WHERE c0 IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert_eq!(
            stored, 0,
            "external-content fts must not duplicate the text"
        );
    }

    #[test]
    fn removing_a_file_leaves_no_trace_in_search() {
        let (_dir, store) = tmp_store();
        indexed_file(&store, "/corpus/a.txt", &["invoice airline"]);
        store.remove_row(1).unwrap();
        assert!(store
            .file_ids_matching(&crate::search::term_match_expr("invoice"))
            .unwrap()
            .is_empty());
        let files: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .unwrap();
        let tiles: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM tiles", [], |r| r.get(0))
            .unwrap();
        assert_eq!((files, tiles), (0, 0), "tiles cascade with the file");
    }

    #[test]
    fn a_move_updates_the_path_and_the_searchable_name_in_one_step() {
        let (_dir, store) = tmp_store();
        indexed_file(&store, "/corpus/a.txt", &["a.txt\ninvoice airline"]);
        // The caller names the new path; re-titling the searchable text is the
        // store's job, so the two cannot be told to disagree.
        store
            .move_file(1, Path::new("/corpus/b.txt"), &facts(24), "abc123")
            .unwrap();
        assert!(store
            .file_state(Path::new("/corpus/a.txt"))
            .unwrap()
            .is_none());
        let (path, _) = store.file_row(1).unwrap();
        assert_eq!(path, "/corpus/b.txt");
        let files: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(files, 1, "a move must not duplicate the row");
        // the new name is searchable, the old one is gone
        assert_eq!(
            store
                .file_ids_matching(&crate::search::term_match_expr("b.txt"))
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .file_ids_matching(&crate::search::term_match_expr("a.txt"))
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .file_ids_matching(&crate::search::term_match_expr("airline"))
                .unwrap()
                .len(),
            1,
            "the extracted text is untouched by a move"
        );
    }

    #[test]
    fn a_move_candidate_is_scoped_to_the_root_and_the_gone_path() {
        let (_dir, store) = tmp_store();
        indexed_file(&store, "/corpus/a.txt", &["same bytes"]);
        store
            .index_file(
                Path::new("/other"),
                Path::new("/other/z.txt"),
                Kind::Text,
                &facts(10),
                Some("abc123"),
                &tiles(&["same bytes"]),
            )
            .unwrap();
        // None of these paths exist on disk, so the first is a move...
        assert_eq!(
            store
                .moved_candidate(Path::new("/corpus"), "abc123", Path::new("/corpus/b.txt"))
                .unwrap(),
            Some((1, "/corpus/a.txt".to_string()))
        );
        // ...a file is never a candidate for moving onto itself...
        assert!(store
            .moved_candidate(Path::new("/corpus"), "abc123", Path::new("/corpus/a.txt"))
            .unwrap()
            .is_none());
        // ...another folder's row is off limits...
        assert!(store
            .moved_candidate(Path::new("/third"), "abc123", Path::new("/third/new.txt"))
            .unwrap()
            .is_none());
        // ...and a row whose path is still on disk is two live files, not a move.
        let live = tempfile::tempdir().unwrap();
        let live_path = live.path().join("a.txt");
        std::fs::write(&live_path, "same bytes").unwrap();
        store
            .index_file(
                live.path(),
                &live_path,
                Kind::Text,
                &facts(10),
                Some("abc123"),
                &tiles(&["same bytes"]),
            )
            .unwrap();
        assert!(
            store
                .moved_candidate(live.path(), "abc123", &live.path().join("copy.txt"))
                .unwrap()
                .is_none(),
            "a file whose old path still exists was treated as a move"
        );
    }

    #[test]
    fn a_skipped_file_keeps_its_reason_and_drops_its_hash() {
        let (_dir, store) = tmp_store();
        indexed_file(&store, "/corpus/a.png", &["a.png\ninvoice"]);
        store
            .skip_file(
                Path::new("/corpus"),
                Path::new("/corpus/a.png"),
                Kind::Image,
                &facts(0),
                SkipReason::CloudPlaceholder,
            )
            .unwrap();
        let state = store
            .file_state(Path::new("/corpus/a.png"))
            .unwrap()
            .unwrap();
        assert!(!state.extracted, "a skipped file is not extracted");
        assert_eq!(state.sha1, None, "nothing was read, so nothing is claimed");
        assert_eq!(
            store.skip_reasons().unwrap(),
            vec![("cloud_placeholder".into(), 1)]
        );
        // what was already known about the file is not thrown away
        assert_eq!(store.tile_texts(1).unwrap(), vec!["a.png\ninvoice"]);
    }

    /// A row that stopped being trustworthy must not be moved by an old hash:
    /// only content this engine actually extracted can claim a new path.
    #[test]
    fn a_failed_file_is_not_a_move_candidate() {
        let (_dir, store) = tmp_store();
        indexed_file(&store, "/corpus/a.txt", &["a.txt\ninvoice"]);
        store
            .fail_file(
                Path::new("/corpus"),
                Path::new("/corpus/a.txt"),
                Kind::Text,
                &facts(10),
                "reading a.txt: invalid utf-8",
            )
            .unwrap();
        let state = store
            .file_state(Path::new("/corpus/a.txt"))
            .unwrap()
            .unwrap();
        assert!(!state.extracted);
        assert_eq!(
            state.sha1, None,
            "a failed extraction vouches for no content"
        );
        assert!(
            store
                .moved_candidate(Path::new("/corpus"), "abc123", Path::new("/corpus/b.txt"))
                .unwrap()
                .is_none(),
            "a failed row was moved by a stale hash"
        );
    }

    #[test]
    fn failure_reasons_stay_out_of_the_skip_breakdown() {
        let (_dir, store) = tmp_store();
        store
            .fail_file(
                Path::new("/corpus"),
                Path::new("/corpus/bad.txt"),
                Kind::Text,
                &facts(4),
                "reading a.txt: invalid utf-8",
            )
            .unwrap();
        store
            .skip_file(
                Path::new("/corpus"),
                Path::new("/corpus/huge.txt"),
                Kind::Text,
                &facts(u64::MAX / 2),
                SkipReason::TooLarge,
            )
            .unwrap();
        assert_eq!(store.skip_reasons().unwrap(), vec![("too_large".into(), 1)]);
        let stats = store.stats().unwrap();
        assert!(stats.contains(&("failed".to_string(), 1)));
        assert!(stats.contains(&("skipped".to_string(), 1)));
    }

    /// The deletion sweep's judgement call, tested where it now lives: absent
    /// is deletion, unreadable is not.
    #[test]
    fn a_gone_path_and_an_unreadable_one_are_different_things() {
        let dir = tempfile::tempdir().unwrap();
        let present = dir.path().join("here.txt");
        std::fs::write(&present, "x").unwrap();
        assert!(!is_gone(&present));
        assert!(is_gone(&dir.path().join("gone.txt")));
    }

    #[test]
    fn future_schema_is_a_loud_error() {
        let (_dir, store) = tmp_store();
        store.conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(store.migrate().is_err());
    }
}
