//! What the desktop shell asks the engine, independent of the windowing toolkit.
//!
//! The shell is thin on purpose: every answer here is a shape a Tauri `invoke`
//! handler (or a native window) can render, and every decision worth testing —
//! result shaping, snippet highlighting, the empty and missing-model states —
//! is here rather than in JavaScript. `tessera-core` is the only engine; this
//! module never reaches around it to the database.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use tessera_core::cli::{image_query_embedder, text_embedder, text_query_embedder};
use tessera_core::fusion::{
    line_marks, missing_model_notes, parse_query, search as fused_search, Mark, SearchMode,
};
use tessera_core::index::backfill::{backfill, BackfillSummary};
use tessera_core::index::{index_folder, index_folder_with_text, IndexSummary};
use tessera_core::models::Models;
use tessera_core::ocr::OcrChoice;
use tessera_core::store::Store;
use tessera_core::vectors::{QueryEmbedder, TextEmbedder};

/// Longest edge of a result thumbnail, in pixels. The wall's rows are about
/// 168 px tall, so 480 leaves detail on a 2x display without holding a
/// screenshot in memory.
const THUMBNAIL_EDGE: u32 = 480;

/// The thumbnail cache's format version. A change to the edge or the encoder
/// moves it, so an old entry is never served for a new size.
const THUMBNAIL_VERSION: u32 = 2;

/// JPEG quality for a thumbnail. 82 is visually clean at this size and keeps a
/// screenshot near 20 KB, which matters when forty rows load at once.
const THUMBNAIL_QUALITY: u8 = 82;

/// A decode guard, independent of the index's 50 MB admission cap (a row
/// indexed before that cap could name a larger file).
const MAX_THUMBNAIL_SOURCE_BYTES: u64 = 64 * 1024 * 1024;

/// The query arms' embedders, loaded once and shared by every query.
///
/// Both constructors verify their model set by hashing it (~1.47 GB) and build
/// the ONNX sessions, so this is the expensive half of a cold search. The trait
/// objects are `Send + Sync`, which is what makes sharing them across the
/// shell's command threads safe.
#[derive(Clone)]
struct SearchEmbedders {
    image: Option<Arc<dyn QueryEmbedder>>,
    text: Option<Arc<dyn TextEmbedder>>,
}

impl std::fmt::Debug for SearchEmbedders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchEmbedders")
            .field("image", &self.image.is_some())
            .field("text", &self.text.is_some())
            .finish()
    }
}

/// One highlighted run of a snippet: `hit` runs are drawn with the match style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub text: String,
    pub hit: bool,
}

/// One result as the list renders it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub kind: String,
    pub page: Option<u32>,
    pub snippet: Vec<Segment>,
    /// `keyword`, `image`, `meaning` — the arms that found this file.
    pub arms: Vec<String>,
    /// The picture's size, for the wall's layout. `None` for a document or a
    /// picture whose header has not been read yet.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The matching lines' places on the picture. Empty for a document, for a
    /// match that came from the image arm alone, and for a picture whose
    /// geometry has not been read.
    pub marks: Vec<RowMark>,
    /// The file's only searchable text is its own name: there is nothing
    /// inside it to show, so the wall and the preview say so.
    pub name_only: bool,
}

/// A rectangle on a picture, in fractions of its width and height. The app's
/// serializable mirror of [`tessera_core::fusion::Mark`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RowMark {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// The home's answer: a page of the library plus the counts that describe the
/// index behind it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryPayload {
    pub rows: Vec<LibraryRow>,
    pub pictures: i64,
    pub documents: i64,
    pub indexed: i64,
    pub folders: usize,
}

/// One library row: a file the wall or the document list can show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryRow {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub kind: String,
    /// Nanoseconds since the Unix epoch, as the last index run saw it.
    pub mtime: i64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// A search answer, including why it may be empty.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchPayload {
    pub rows: Vec<Row>,
    /// User-facing notes: no index, no models, an arm with nothing to rank.
    pub notes: Vec<String>,
    /// The whole command, from entry: model load on a cold query plus search.
    /// This is the number the window shows, because it is the wait the user
    /// just sat through.
    pub millis: u64,
    /// The fused search alone, after the embedders are ready. Kept separate so
    /// a slow first query can be told apart from a slow search.
    pub engine_millis: u64,
    pub arms: Vec<String>,
    /// The lens that answered. The window drops a response whose mode is not
    /// the lens on screen, so a slow answer cannot repaint a new question.
    pub mode: String,
}

/// The preview pane's answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Preview {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub page: Option<u32>,
    /// The text of the matched tile (OCR text for an image), with the query's
    /// terms highlighted.
    pub text: Vec<Segment>,
    /// Present for images: the shell displays this file directly.
    pub image_path: Option<String>,
    /// The matching lines' places on the picture, in the same fractions the
    /// wall uses. Empty for a document and for a query that marks nothing.
    pub marks: Vec<RowMark>,
}

/// Counts and paths for the progress and settings screens.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusPayload {
    pub index_path: String,
    pub index_mb: f64,
    /// Kept until the old window is replaced; Settings reads `folder_rows`,
    /// which also carries each folder's file count.
    pub folders: Vec<String>,
    pub folder_rows: Vec<FolderCountRow>,
    pub version: String,
    /// `(status, count)`, from the store.
    pub counts: Vec<(String, i64)>,
    /// `(kind, count)` among indexed files.
    pub counts_by_kind: Vec<(String, i64)>,
    pub failures: Vec<FailureRow>,
    pub skip_reasons: Vec<(String, i64)>,
    /// `(set, complete, missing files)`.
    pub models: Vec<ModelSetRow>,
    pub image_vectors: i64,
    pub text_vectors: i64,
    /// The OCR engine this build prefers, for Settings.
    pub ocr_engine: String,
    /// Pictures whose line geometry has not been read yet. Settings shows one
    /// quiet line when this is above zero.
    pub highlight_pending: i64,
}

/// One indexed folder and how many files it holds, for Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderCountRow {
    pub path: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FailureRow {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelSetRow {
    pub name: String,
    pub complete: bool,
    pub missing: Vec<String>,
}

/// The empty-query state's answer: the newest files the index holds, with the
/// counts that describe the index behind them.
///
/// Kept apart from [`StatusPayload`] because the two are asked at different
/// rates and for different panes: the home state refreshes while a folder is
/// being indexed, while `status` collects failures, skip reasons and model
/// verification for Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecentPayload {
    pub rows: Vec<HomeRow>,
    /// Rows whose content was extracted. Skipped and failed rows are work
    /// still waiting, not files the user can open.
    pub indexed: i64,
    pub folders: usize,
}

/// One newest file as the home list renders it. No snippet and no arms: this
/// list answers "what changed lately", not "why did this match".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HomeRow {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub kind: String,
    /// Nanoseconds since the Unix epoch, as the last index run saw it. The
    /// window formats this; the engine does not know the user's locale.
    pub mtime: i64,
}

/// The engine as the shell sees it. Cheap to clone: two paths.
#[derive(Debug, Clone)]
pub struct AppService {
    data_dir: PathBuf,
    models_dir: PathBuf,
    /// Model state, computed once per process.
    ///
    /// `Models::status` verifies every model file by hashing it, which is
    /// gigabytes of SHA-256 — far too expensive for a call the window makes on
    /// a timer. The answer only changes when an install finishes, so it is
    /// cached here and dropped there.
    verified_models: Arc<Mutex<Option<Vec<ModelSetRow>>>>,
    /// The query embedders, loaded and verified once per process.
    ///
    /// They used to be rebuilt on every search, which re-hashed every model
    /// file and re-created both ONNX sessions per keystroke. The answer only
    /// changes on an install, so it shares the status cache's lifetime.
    search_embedders: Arc<Mutex<Option<SearchEmbedders>>>,
    /// How many times the embedders were constructed. Test-only: it is the
    /// observable that proves a second query reuses them.
    #[cfg(test)]
    search_loads: Arc<std::sync::atomic::AtomicUsize>,
    /// How many times a thumbnail was decoded rather than read from the cache.
    /// Test-only, for the same reason as `search_loads`.
    #[cfg(test)]
    thumbnail_decodes: Arc<std::sync::atomic::AtomicUsize>,
}

impl AppService {
    pub fn new(data_dir: impl Into<PathBuf>, models_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            models_dir: models_dir.into(),
            verified_models: Arc::new(Mutex::new(None)),
            search_embedders: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            search_loads: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            #[cfg(test)]
            thumbnail_decodes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    /// Whether an index file exists at all. The shell uses this to show the
    /// first-run state instead of an empty result list that looks like a miss.
    pub fn index_exists(&self) -> bool {
        self.data_dir.join("index.sqlite").exists()
    }

    /// Run the one search box under one lens: the arms that lens runs, fused.
    ///
    /// `millis` times the whole command from this line, including the one-time
    /// model load; `engine_millis` is the fused search alone. The window shows
    /// both, so the first query after launch does not claim to have been fast.
    pub fn search(
        &self,
        query: &str,
        mode: SearchMode,
        limit: usize,
    ) -> Result<SearchPayload, String> {
        let started = std::time::Instant::now();
        if !self.index_exists() {
            return Err("no index yet: add a folder to start indexing".into());
        }
        let parsed = parse_query(&[query.to_string()]).map_err(|err| format!("{err:#}"))?;
        if parsed.terms.is_empty() && parsed.filter.is_empty() {
            return Ok(SearchPayload {
                rows: Vec::new(),
                notes: vec!["Type a word, or a filter such as type:pdf.".into()],
                millis: started.elapsed().as_millis() as u64,
                engine_millis: 0,
                arms: Vec::new(),
                mode: mode.name().to_string(),
            });
        }
        let embedders = self.search_embedders()?;
        let mut notes =
            missing_model_notes(mode, embedders.image.is_some(), embedders.text.is_some());
        let engine_started = std::time::Instant::now();
        let outcome = fused_search(
            &self.data_dir,
            &parsed,
            mode,
            embedders.image.as_deref(),
            embedders.text.as_deref(),
            limit.clamp(1, 100),
        )
        .map_err(|err| format!("{err:#}"))?;
        let engine_millis = engine_started.elapsed().as_millis() as u64;
        notes.extend(outcome.notes.iter().cloned());
        let terms: Vec<String> = parsed.terms.clone();
        let rows = outcome
            .hits
            .iter()
            .map(|hit| row_from_hit(hit, &terms))
            .collect();
        let arms = outcome
            .timings
            .iter()
            .map(|timing| timing.arm.name().to_string())
            .collect();
        Ok(SearchPayload {
            rows,
            notes,
            millis: started.elapsed().as_millis() as u64,
            engine_millis,
            arms,
            mode: mode.name().to_string(),
        })
    }

    /// The query embedders, loaded and verified once per process.
    ///
    /// `Models::open` plus the two constructors hash every file in both sets
    /// (about 1.47 GB) and build the ONNX sessions. Doing that per query made
    /// two searches in one process take 46.2 s and 51.0 s in debug while the
    /// payload reported 1.5 s. The lock is held across the load, so two cold
    /// queries do not both pay for it; the clone is released before the search
    /// and the sessions are safe to share.
    fn search_embedders(&self) -> Result<SearchEmbedders, String> {
        let mut cache = self
            .search_embedders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(embedders) = cache.as_ref() {
            return Ok(embedders.clone());
        }
        let models = Models::open(&self.models_dir).map_err(|err| format!("{err:#}"))?;
        let embedders = SearchEmbedders {
            image: image_query_embedder(&models)
                .map(|embedder| -> Arc<dyn QueryEmbedder> { embedder }),
            text: text_query_embedder(&models)
                .map(|embedder| -> Arc<dyn TextEmbedder> { Arc::new(embedder) }),
        };
        #[cfg(test)]
        self.search_loads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *cache = Some(embedders.clone());
        Ok(embedders)
    }

    /// The preview pane: the matched tile's text, highlighted, the image path
    /// when the hit is an image, and where on the picture the matching lines
    /// sit. A file that has no tile (an image whose OCR produced nothing) still
    /// previews as an image.
    ///
    /// `page` is the page the result named. A stale number — the file was
    /// reindexed since the search — falls back to the first tile rather than
    /// an error: the row is out of date, not the file.
    pub fn preview(&self, path: &str, query: &str, page: Option<u32>) -> Result<Preview, String> {
        let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
        let file_id = store
            .file_id(Path::new(path))
            .map_err(|err| format!("{err:#}"))?;
        let mut files = store
            .files_by_ids(&[file_id])
            .map_err(|err| format!("{err:#}"))?;
        let file = files
            .pop()
            .ok_or_else(|| "that file is not in the index".to_string())?;
        let terms: Vec<String> = parse_query(&[query.to_string()])
            .map(|parsed| parsed.terms)
            .unwrap_or_default();
        let chosen = page
            .and_then(|wanted| file.tiles.iter().find(|tile| tile.page == Some(wanted)))
            .or_else(|| file.tiles.first());
        let page = chosen.and_then(|tile| tile.page);
        let text = chosen
            .map(|tile| highlight(&tile.text, &terms))
            .unwrap_or_default();
        let is_image = file.kind == "image";
        let marks = if is_image {
            let needles: Vec<&str> = terms.iter().map(String::as_str).collect();
            chosen
                .map(|tile| row_marks(&line_marks(&tile.text, &tile.boxes, &needles)))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(Preview {
            path: file.path.clone(),
            name: file_name(&file.path),
            kind: file.kind,
            page,
            text,
            image_path: is_image.then_some(file.path),
            marks,
        })
    }

    /// The bytes of an indexed image, for the preview pane.
    ///
    /// The asset protocol is deliberately disabled: the window can read a file
    /// only through this command, and only when the index says the path is an
    /// indexed image. That keeps "the webview can read any file the user names"
    /// from being true while still letting a preview show a screenshot from
    /// anywhere the user chose to index.
    pub fn image_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        if !self.index_exists() {
            return Err("no index yet: add a folder to start indexing".into());
        }
        let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
        let file_id = store
            .file_id(Path::new(path))
            .map_err(|_| "that file is not in the index".to_string())?;
        let file = store
            .files_by_ids(&[file_id])
            .map_err(|err| format!("{err:#}"))?
            .into_iter()
            .next()
            .ok_or_else(|| "that file is not in the index".to_string())?;
        if file.kind != "image" {
            return Err("only image hits are previewed as pictures".into());
        }
        let meta = std::fs::metadata(path).map_err(|err| format!("reading {path}: {err}"))?;
        // A preview is not a download: refuse a file large enough to stall the
        // window rather than trying to decode it into a web view.
        if meta.len() > 32 * 1024 * 1024 {
            return Err("that image is larger than 32 MB; open it in its own app".into());
        }
        std::fs::read(path).map_err(|err| format!("reading {path}: {err}"))
    }

    /// A small cached picture of an indexed image, for the result list.
    ///
    /// Same reach as [`AppService::image_bytes`] — the index must know the
    /// path as an image — but a different job: a 40-row list cannot hold 40
    /// full screenshots in memory, so this decodes once, shrinks the longest
    /// edge to [`THUMBNAIL_EDGE`] px, and keeps the JPEG under
    /// `<data_dir>/thumbs/`. A later request for the same file version is a
    /// file read, not a decode.
    ///
    /// The key is the file id plus the mtime of the bytes on disk, so an edit
    /// that lands before the watcher re-indexes it cannot serve the old
    /// picture. This is deliberately not on the search path: the list renders
    /// from the payload first and asks for pictures afterwards.
    ///
    /// Errors are the three facts the user can act on — not an image, not on
    /// disk, or not decodable — plus the no-index first-run case.
    pub fn thumbnail(&self, path: &str) -> Result<Vec<u8>, String> {
        if !self.index_exists() {
            return Err("no index yet: add a folder to start indexing".into());
        }
        let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
        let source = store
            .thumb_source(Path::new(path))
            .map_err(|err| format!("{err:#}"))?
            .ok_or_else(|| "that file is not in the index".to_string())?;
        if source.kind != "image" {
            return Err("thumbnails are only made for image hits".into());
        }
        let target = Path::new(path);
        let meta = std::fs::metadata(target).map_err(|err| format!("reading {path}: {err}"))?;
        if !meta.is_file() {
            return Err(format!("{path} is not a file"));
        }
        // The index stops at 50 MB; this is a wider guard for a row indexed
        // before that cap, and it refuses before a byte is read.
        if meta.len() > MAX_THUMBNAIL_SOURCE_BYTES {
            return Err(format!(
                "that image is larger than {} MB; open it in its own app",
                MAX_THUMBNAIL_SOURCE_BYTES / (1024 * 1024)
            ));
        }
        let mtime = disk_mtime_nanos(&meta).unwrap_or(source.indexed_mtime as u128);
        let key = format!("{}-{mtime}-v{THUMBNAIL_VERSION}.jpg", source.file_id);
        let cache_dir = self.data_dir.join("thumbs");
        let cache_file = cache_dir.join(&key);
        if let Ok(bytes) = std::fs::read(&cache_file) {
            if !bytes.is_empty() {
                return Ok(bytes);
            }
        }

        let bytes = render_thumbnail(target)?;
        #[cfg(test)]
        self.thumbnail_decodes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // Both of these describe work already done: a cache that cannot be
        // written still has an answer to return, so neither failure denies the
        // user their picture.
        if let Err(err) = write_cache(&cache_dir, &cache_file, &bytes) {
            eprintln!("the thumbnail cache could not be written: {err}");
        }
        if let Err(err) = store.set_thumb(source.file_id, Some(&key)) {
            eprintln!("the thumbnail record could not be written: {err:#}");
        }
        Ok(bytes)
    }

    /// Forget one indexed folder. The files on disk are untouched; only the
    /// index rows, tiles and vectors that describe them go.
    ///
    /// Returns the number of rows removed, so Settings can say what changed.
    pub fn remove_root(&self, path: &str) -> Result<usize, String> {
        if !self.index_exists() {
            return Ok(0);
        }
        let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
        store
            .remove_root(Path::new(path))
            .map_err(|err| format!("{err:#}"))
    }

    /// Delete the thumbnail cache and the index records that name its entries.
    ///
    /// Returns how many cache files were removed. The pictures on disk and
    /// their index rows stay; the next wall request decodes them again.
    pub fn clear_thumbnails(&self) -> Result<usize, String> {
        let dir = self.data_dir.join("thumbs");
        let mut removed = 0;
        match std::fs::read_dir(&dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|err| format!("reading {}: {err}", dir.display()))?;
                    let path = entry.path();
                    if path.is_file() {
                        std::fs::remove_file(&path)
                            .map_err(|err| format!("removing {}: {err}", path.display()))?;
                        removed += 1;
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("reading {}: {err}", dir.display())),
        }
        if self.index_exists() {
            let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
            store.clear_thumbs().map_err(|err| format!("{err:#}"))?;
        }
        Ok(removed)
    }

    pub fn status(&self) -> Result<StatusPayload, String> {
        let index_path = self.data_dir.join("index.sqlite");
        let index_mb = std::fs::metadata(&index_path)
            .map(|meta| meta.len() as f64 / 1_048_576.0)
            .unwrap_or(0.0);
        let mut payload = StatusPayload {
            index_path: index_path.display().to_string(),
            index_mb: (index_mb * 10.0).round() / 10.0,
            folders: Vec::new(),
            folder_rows: Vec::new(),
            version: tessera_core::VERSION.to_string(),
            counts: Vec::new(),
            counts_by_kind: Vec::new(),
            failures: Vec::new(),
            skip_reasons: Vec::new(),
            models: Vec::new(),
            image_vectors: 0,
            text_vectors: 0,
            ocr_engine: tessera_core::ocr::default_engine_name().to_string(),
            highlight_pending: 0,
        };
        if !self.index_exists() {
            payload.models = self.model_rows();
            return Ok(payload);
        }
        let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
        let roots = store.roots().map_err(|err| format!("{err:#}"))?;
        let root_counts: std::collections::HashMap<String, i64> = store
            .root_counts()
            .map_err(|err| format!("{err:#}"))?
            .into_iter()
            .collect();
        payload.folder_rows = roots
            .into_iter()
            .map(|root| {
                let path = root.display().to_string();
                let count = root_counts.get(&path).copied().unwrap_or(0);
                FolderCountRow { path, count }
            })
            .collect();
        payload.folders = payload
            .folder_rows
            .iter()
            .map(|row| row.path.clone())
            .collect();
        payload.counts = store.stats().map_err(|err| format!("{err:#}"))?;
        payload.counts_by_kind = store.kind_counts().map_err(|err| format!("{err:#}"))?;
        payload.highlight_pending = store
            .image_tiles_missing_boxes_count()
            .map_err(|err| format!("{err:#}"))?;
        payload.failures = store
            .failures()
            .map_err(|err| format!("{err:#}"))?
            .into_iter()
            .map(|(path, error)| FailureRow { path, error })
            .collect();
        payload.skip_reasons = store.skip_reasons().map_err(|err| format!("{err:#}"))?;
        payload.image_vectors = store
            .image_embedding_count()
            .map_err(|err| format!("{err:#}"))?;
        payload.text_vectors = store
            .text_embedding_count()
            .map_err(|err| format!("{err:#}"))?;
        payload.models = self.model_rows();
        Ok(payload)
    }

    /// The newest files in the index, for the window's empty-query state.
    ///
    /// `limit` is clamped here as well as in the window: a caller cannot turn
    /// the home list into a full-index read. Counts come from the same
    /// connection, so the list and "N files" describe one consistent index.
    pub fn recent(&self, limit: usize) -> Result<RecentPayload, String> {
        if !self.index_exists() {
            return Ok(RecentPayload {
                rows: Vec::new(),
                indexed: 0,
                folders: 0,
            });
        }
        let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
        let folders = store.roots().map_err(|err| format!("{err:#}"))?.len();
        let indexed = store
            .stats()
            .map_err(|err| format!("{err:#}"))?
            .into_iter()
            .find(|(status, _)| status == "indexed")
            .map(|(_, count)| count)
            .unwrap_or(0);
        let rows = store
            .recent_files(limit.clamp(1, 24))
            .map_err(|err| format!("{err:#}"))?
            .into_iter()
            .map(|file| HomeRow {
                name: file_name(&file.path),
                folder: folder_of(&file.path),
                path: file.path,
                kind: file.kind,
                mtime: file.mtime,
            })
            .collect();
        Ok(RecentPayload {
            rows,
            indexed,
            folders,
        })
    }

    /// The library: the newest indexed files, optionally narrowed to some
    /// kinds, with the counts that describe the whole index.
    ///
    /// The counts are deliberately not filtered by `kinds`: the home's "12
    /// pictures, 40 documents" describes the library, not the page it is
    /// showing. `limit` is clamped here so a caller cannot turn the wall into
    /// a full-index read.
    pub fn library(&self, kinds: &[String], limit: usize) -> Result<LibraryPayload, String> {
        if !self.index_exists() {
            return Ok(LibraryPayload {
                rows: Vec::new(),
                pictures: 0,
                documents: 0,
                indexed: 0,
                folders: 0,
            });
        }
        let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
        let rows = store
            .library_files(kinds, limit.clamp(1, 400))
            .map_err(|err| format!("{err:#}"))?
            .into_iter()
            .map(|file| LibraryRow {
                name: file_name(&file.path),
                folder: folder_of(&file.path),
                path: file.path,
                kind: file.kind,
                mtime: file.mtime,
                width: file.width,
                height: file.height,
            })
            .collect();
        let counts = store.kind_counts().map_err(|err| format!("{err:#}"))?;
        let count_of = |kind: &str| {
            counts
                .iter()
                .filter(|(name, _)| name == kind)
                .map(|(_, count)| *count)
                .sum::<i64>()
        };
        let folders = store.roots().map_err(|err| format!("{err:#}"))?.len();
        Ok(LibraryPayload {
            rows,
            pictures: count_of("image"),
            documents: count_of("pdf") + count_of("docx") + count_of("text"),
            indexed: counts.iter().map(|(_, count)| count).sum(),
            folders,
        })
    }

    fn model_rows(&self) -> Vec<ModelSetRow> {
        let mut cache = self
            .verified_models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(rows) = cache.as_ref() {
            return rows.clone();
        }
        let rows: Vec<ModelSetRow> = Models::open(&self.models_dir)
            .and_then(|models| models.status())
            .map(|sets| {
                sets.into_iter()
                    .map(|set| {
                        let complete = set.is_complete();
                        ModelSetRow {
                            name: set.name,
                            complete,
                            missing: set.missing,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        *cache = Some(rows.clone());
        rows
    }

    /// Whether the model verification has run (and is therefore cached).
    #[cfg(test)]
    fn models_are_cached(&self) -> bool {
        self.verified_models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    /// How many times search built the embedders in this process.
    #[cfg(test)]
    fn search_loads(&self) -> usize {
        self.search_loads.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many thumbnails this process decoded (as opposed to re-read from
    /// the cache).
    #[cfg(test)]
    fn thumbnail_decodes(&self) -> usize {
        self.thumbnail_decodes
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Whether the query embedders are cached.
    #[cfg(test)]
    fn search_embedders_are_cached(&self) -> bool {
        self.search_embedders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    /// Fill the picture sizes and line boxes that rows indexed before the
    /// geometry upgrade do not have.
    ///
    /// Runs once per launch on a background thread. It reads only indexed
    /// image paths, never a cloud placeholder, and does nothing before the
    /// first folder exists. A resolved row is not in the work list again.
    pub fn backfill_missing_geometry(&self) -> Result<BackfillSummary, String> {
        if !self.index_exists() {
            return Ok(BackfillSummary::default());
        }
        let models = Models::open(&self.models_dir).map_err(|err| format!("{err:#}"))?;
        let ocr = tessera_core::cli::ocr_engine(&OcrChoice::Native, &models)
            .map_err(|err| format!("{err:#}"))?;
        let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
        let summary = backfill(&store, ocr.as_ref()).map_err(|err| format!("{err:#}"))?;
        Ok(summary)
    }

    /// Add a folder by indexing it. One-shot, so the shell can run it on a
    /// worker thread and report the summary; the watcher is started separately.
    pub fn index_folder(&self, path: &str) -> Result<String, String> {
        let folder = Path::new(path);
        if !folder.is_dir() {
            return Err(format!("{} is not a folder", folder.display()));
        }
        let models = Models::open(&self.models_dir).map_err(|err| format!("{err:#}"))?;
        let pipelines = tessera_core::cli::pipelines(&OcrChoice::Native, &models)
            .map_err(|err| format!("{err:#}"))?;
        let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
        let text = text_embedder(&models);
        let summary: IndexSummary = match text.as_ref() {
            Some(text) => index_folder_with_text(&store, &pipelines, folder, text),
            None => index_folder(&store, &pipelines, folder),
        }
        .map_err(|err| format!("{err:#}"))?;
        Ok(format!(
            "{} indexed, {} unchanged, {} skipped, {} failed",
            summary.indexed, summary.unchanged, summary.skipped, summary.failed
        ))
    }

    /// Confirm-and-rebuild: move the index file aside and let the next index
    /// run build a fresh one. The user's folders are re-read from the store
    /// before it closes, so the confirmation screen can say what is about to
    /// be re-indexed.
    pub fn rebuild_index(&self) -> Result<Vec<String>, String> {
        let folders = if self.index_exists() {
            Store::open(&self.data_dir)
                .and_then(|store| store.roots())
                .map(|roots| {
                    roots
                        .into_iter()
                        .map(|path| path.display().to_string())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for suffix in ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"] {
            let path = self.data_dir.join(suffix);
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|err| format!("removing {}: {err}", path.display()))?;
            }
        }
        Ok(folders)
    }

    /// The model download state, without opening the index.
    pub fn models(&self) -> Vec<ModelSetRow> {
        self.model_rows()
    }

    /// Fill the pinned model sets. The only operation in the app that touches
    /// the network, and only when the user asks for it.
    pub fn install_models(&self, sets: &[String]) -> Result<Vec<String>, String> {
        let models = Models::open(&self.models_dir).map_err(|err| format!("{err:#}"))?;
        let wanted: Vec<&str> = sets.iter().map(String::as_str).collect();
        let report = models.ensure(&wanted).map_err(|err| format!("{err:#}"))?;
        // The install changed the very files both caches describe: the status
        // answer, and the loaded query embedders built from them.
        self.forget_models();
        Ok(report.lines())
    }

    /// Drop every answer that was computed from the model files.
    fn forget_models(&self) {
        *self
            .verified_models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .search_embedders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    /// The image arm's own vector count, used by the empty-state copy to say
    /// "indexed, but descriptions were not built".
    pub fn image_vectors(&self) -> i64 {
        if !self.index_exists() {
            return 0;
        }
        Store::open(&self.data_dir)
            .and_then(|store| store.image_embedding_count())
            .unwrap_or(0)
    }
}

/// The mtime of the bytes on disk, in nanoseconds since the Unix epoch.
///
/// `None` when the platform will not report it; the caller then falls back to
/// the mtime the index recorded rather than collapsing every version onto one
/// cache key.
fn disk_mtime_nanos(meta: &std::fs::Metadata) -> Option<u128> {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_nanos())
}

/// Decode an image, shrink its longest edge to [`THUMBNAIL_EDGE`], and encode
/// it as JPEG.
///
/// The decode itself is the engine's (`tessera_core::thumb`), which caps the
/// pixels one file may allocate; this function only fixes the size and quality
/// the list wants and makes the error text name the file.
fn render_thumbnail(path: &Path) -> Result<Vec<u8>, String> {
    tessera_core::thumb::render_jpeg(path, THUMBNAIL_EDGE, THUMBNAIL_QUALITY)
        .map_err(|err| format!("{err:#}"))
}

/// Write a cache entry through a temp file and a rename.
///
/// Two rows can ask for the same thumbnail at once (a double render, two
/// windows); writing in place would let one reader observe half a file. The
/// rename replaces atomically enough for a cache: a reader sees the old entry
/// or the new one, never a mix.
fn write_cache(dir: &Path, target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static WRITES: AtomicU64 = AtomicU64::new(0);
    std::fs::create_dir_all(dir)?;
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "thumb".to_string());
    let temp = dir.join(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::write(&temp, bytes)?;
    if let Err(err) = std::fs::rename(&temp, target) {
        let _ = std::fs::remove_file(&temp);
        return Err(err);
    }
    Ok(())
}

/// The lens named by a command argument, or the default when absent.
///
/// The window sends a lens name rather than an enum so a typo is a clear
/// error, not a silent fallback to `all` that mixes every kind again.
pub fn parse_lens(mode: Option<&str>) -> Result<SearchMode, String> {
    match mode {
        None => Ok(SearchMode::All),
        Some(name) => SearchMode::parse(name)
            .ok_or_else(|| format!("{name} is not a lens — use all, pictures, words or documents")),
    }
}

/// A fused hit as the result list wants it.
fn row_from_hit(hit: &tessera_core::fusion::FusedHit, terms: &[String]) -> Row {
    Row {
        path: hit.path.clone(),
        name: file_name(&hit.path),
        folder: folder_of(&hit.path),
        kind: hit.kind.clone(),
        page: hit.page,
        snippet: highlight(&hit.snippet, terms),
        arms: hit.arms.names().into_iter().map(str::to_string).collect(),
        width: hit.width,
        height: hit.height,
        marks: row_marks(&hit.marks),
        name_only: hit.name_only,
    }
}

fn row_marks(marks: &[Mark]) -> Vec<RowMark> {
    marks
        .iter()
        .map(|mark| RowMark {
            x0: mark.x0,
            y0: mark.y0,
            x1: mark.x1,
            y1: mark.y1,
        })
        .collect()
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn folder_of(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_default()
}

/// Split `text` into runs, marking every occurrence of any of `terms`
/// case-insensitively. Overlapping occurrences merge, and a term that occurs
/// inside a longer match cannot split it. This is the preview and result
/// highlighting; doing it here means the web view never slices a string by an
/// offset the engine did not measure.
pub fn highlight(text: &str, terms: &[String]) -> Vec<Segment> {
    let needles: Vec<String> = terms
        .iter()
        .map(|term| term.to_lowercase())
        .filter(|term| !term.is_empty())
        .collect();
    if needles.is_empty() {
        return vec![Segment {
            text: text.to_string(),
            hit: false,
        }];
    }
    let haystack = text.to_lowercase();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for needle in &needles {
        let mut from = 0;
        while let Some(found) = haystack[from..].find(needle) {
            let start = from + found;
            let end = start + needle.len();
            ranges.push((start, end));
            from = end;
        }
    }
    if ranges.is_empty() {
        return vec![Segment {
            text: text.to_string(),
            hit: false,
        }];
    }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let mut segments = Vec::new();
    let mut cursor = 0;
    for (start, end) in merged {
        if start > cursor {
            segments.push(Segment {
                text: text[cursor..start].to_string(),
                hit: false,
            });
        }
        segments.push(Segment {
            text: text[start..end].to_string(),
            hit: true,
        });
        cursor = end;
    }
    if cursor < text.len() {
        segments.push(Segment {
            text: text[cursor..].to_string(),
            hit: false,
        });
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use tessera_core::geometry::NormBox;
    use tessera_core::index::{Kind, Tile};

    #[test]
    fn a_match_is_highlighted_and_the_rest_is_not() {
        let segments = highlight("The invoice from the airline", &["invoice".into()]);
        assert_eq!(
            segments,
            vec![
                Segment {
                    text: "The ".into(),
                    hit: false
                },
                Segment {
                    text: "invoice".into(),
                    hit: true
                },
                Segment {
                    text: " from the airline".into(),
                    hit: false
                },
            ]
        );
    }

    #[test]
    fn matching_is_case_insensitive_and_repeated() {
        let segments = highlight("Invoice, INVOICE and invoice", &["invoice".into()]);
        let hits: Vec<&str> = segments
            .iter()
            .filter(|segment| segment.hit)
            .map(|segment| segment.text.as_str())
            .collect();
        assert_eq!(hits, vec!["Invoice", "INVOICE", "invoice"]);
    }

    #[test]
    fn overlapping_terms_merge_into_one_run() {
        let segments = highlight("airline invoice", &["air".into(), "line invoice".into()]);
        assert_eq!(
            segments,
            vec![Segment {
                text: "airline invoice".into(),
                hit: true
            }]
        );
    }

    #[test]
    fn no_term_or_no_match_is_one_plain_run() {
        assert_eq!(
            highlight("nothing here", &[]),
            vec![Segment {
                text: "nothing here".into(),
                hit: false
            }]
        );
        assert_eq!(
            highlight("nothing here", &["zebra".into()]),
            vec![Segment {
                text: "nothing here".into(),
                hit: false
            }]
        );
    }

    #[test]
    fn highlighting_is_ut8_safe_and_does_not_drop_text() {
        // The offsets are byte offsets into a lower-cased copy of the same
        // length; this pins that a multi-byte character before the match does
        // not slice it mid-character.
        let text = "café invoice — naïve résumé";
        let segments = highlight(text, &["invoice".into()]);
        let rebuilt: String = segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect();
        assert_eq!(rebuilt, text);
        assert!(segments.iter().any(|segment| segment.hit));
    }

    #[test]
    fn a_row_carries_the_name_folder_and_arms() {
        let hit = tessera_core::fusion::FusedHit {
            file_id: 1,
            path: r"C:\Users\me\Documents\invoice.pdf".into(),
            kind: "pdf".into(),
            page: Some(3),
            snippet: "the invoice total".into(),
            arms: Default::default(),
            name_only: false,
            width: None,
            height: None,
            marks: Vec::new(),
        };
        let row = row_from_hit(&hit, &["invoice".into()]);
        assert_eq!(row.name, "invoice.pdf");
        assert_eq!(row.folder, r"C:\Users\me\Documents");
        assert_eq!(row.kind, "pdf");
        assert_eq!(row.page, Some(3));
        assert!(row.arms.is_empty());
        assert!(row.snippet.iter().any(|segment| segment.hit));
    }

    #[test]
    fn a_missing_index_is_a_first_run_state_not_a_crash() {
        let temp = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        assert!(!service.index_exists());
        let error = service.search("invoice", SearchMode::All, 10).unwrap_err();
        assert!(error.contains("no index yet"), "{error}");
        let status = service.status().unwrap();
        assert_eq!(status.index_mb, 0.0);
        assert!(status.folders.is_empty());
    }

    #[test]
    fn a_path_that_is_not_indexed_is_not_readable_as_an_image() {
        // The preview pane is the only way the web view can read a file. This is
        // the check that keeps it from being a general file reader: the index
        // must know the path before a byte is returned.
        let temp = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let error = service.image_bytes(r"C:\secret\notes.txt").unwrap_err();
        assert!(error.contains("no index yet"), "{error}");

        // An index that exists but does not contain the path is still a refusal,
        // and the message says which fact was missing.
        drop(Store::open(temp.path()).unwrap());
        let error = service.image_bytes(r"C:\secret\notes.png").unwrap_err();
        assert!(error.contains("not in the index"), "{error}");
    }

    // -- library ----------------------------------------------------------

    #[test]
    fn library_returns_rows_and_counts() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));

        // No index is an empty library, not an error: the home state has to be
        // able to ask before the first folder is added.
        let empty = service.library(&[], 40).unwrap();
        assert!(empty.rows.is_empty());
        assert_eq!(
            (
                empty.pictures,
                empty.documents,
                empty.indexed,
                empty.folders
            ),
            (0, 0, 0, 0)
        );

        let shot = corpus.path().join("samurai.png");
        png(&shot, 800, 400);
        let notes = corpus.path().join("notes.txt");
        std::fs::write(&notes, b"words").unwrap();
        let paper = corpus.path().join("invoice.pdf");
        std::fs::write(&paper, b"not really a pdf").unwrap();
        index_with_tiles(
            &service,
            corpus.path(),
            &shot,
            Kind::Image,
            &[picture_tile("samurai.png", "a figure by the bridge")],
        );
        index_as(&service, corpus.path(), &notes, Kind::Text);
        index_as(&service, corpus.path(), &paper, Kind::Pdf);

        let payload = service.library(&[], 40).unwrap();
        assert_eq!(payload.rows.len(), 3);
        assert_eq!(payload.pictures, 1);
        assert_eq!(payload.documents, 2, "pdf and text are documents");
        assert_eq!(payload.indexed, 3);
        assert_eq!(payload.folders, 1);

        // The counts describe the whole index; the kinds only narrow the rows.
        let pictures = service.library(&["image".into()], 40).unwrap();
        assert_eq!(pictures.rows.len(), 1);
        assert_eq!(pictures.rows[0].name, "samurai.png");
        assert_eq!(pictures.pictures, 1);
        assert_eq!(pictures.documents, 2);

        let one = service.library(&[], 1).unwrap();
        assert_eq!(one.rows.len(), 1, "the limit is the window's page size");
    }

    #[test]
    fn rows_carry_picture_size() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let path = corpus.path().join("wide.png");
        png(&path, 800, 400);
        index_as(&service, corpus.path(), &path, Kind::Image);
        set_size(&service, &path, 800, 400);

        let payload = service.library(&[], 40).unwrap();
        assert_eq!(payload.rows[0].width, Some(800));
        assert_eq!(payload.rows[0].height, Some(400));
    }

    #[test]
    fn rows_carry_marks_for_the_matched_lines() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let path = corpus.path().join("samurai.png");
        png(&path, 800, 400);
        index_with_tiles(
            &service,
            corpus.path(),
            &path,
            Kind::Image,
            &[picture_tile(
                "samurai.png",
                "a samurai stands by the bridge",
            )],
        );
        set_size(&service, &path, 800, 400);

        let payload = service.search("samurai", SearchMode::Words, 10).unwrap();
        let row = payload.rows.first().expect("the picture has the word");
        assert_eq!(row.width, Some(800));
        assert_eq!(row.height, Some(400));
        assert!(!row.name_only, "the picture has text beyond its name");
        assert_eq!(row.marks.len(), 1, "{:?}", row.marks);
        let mark = &row.marks[0];
        assert_eq!(
            (mark.x0, mark.y0, mark.x1, mark.y1),
            (0.1, 0.25, 0.6, 0.4),
            "the mark is the line that carried the word"
        );
    }

    // -- preview ----------------------------------------------------------

    #[test]
    fn preview_opens_the_matched_page() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let path = corpus.path().join("report.pdf");
        std::fs::write(&path, b"not really a pdf").unwrap();
        index_with_tiles(
            &service,
            corpus.path(),
            &path,
            Kind::Pdf,
            &[
                Tile {
                    seq: 0,
                    text: "page twelve".into(),
                    page: Some(12),
                    boxes: Vec::new(),
                },
                Tile {
                    seq: 1,
                    text: "page thirteen".into(),
                    page: Some(13),
                    boxes: Vec::new(),
                },
            ],
        );

        let matched = service
            .preview(&path.to_string_lossy(), "", Some(13))
            .unwrap();
        assert_eq!(matched.page, Some(13));
        let text: String = matched.text.iter().map(|s| s.text.as_str()).collect();
        assert!(text.contains("thirteen"), "{text}");

        // A page the file no longer has falls back to the first tile rather
        // than an error: a stale row is not a broken file.
        let stale = service
            .preview(&path.to_string_lossy(), "", Some(99))
            .unwrap();
        assert_eq!(stale.page, Some(12));
        let text: String = stale.text.iter().map(|s| s.text.as_str()).collect();
        assert!(text.contains("twelve"), "{text}");
    }

    #[test]
    fn preview_marks_use_the_query_terms() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let path = corpus.path().join("samurai.png");
        png(&path, 800, 400);
        index_with_tiles(
            &service,
            corpus.path(),
            &path,
            Kind::Image,
            &[picture_tile(
                "samurai.png",
                "a SAMURAI stands by the bridge",
            )],
        );

        let preview = service
            .preview(&path.to_string_lossy(), "samurai", None)
            .unwrap();
        assert_eq!(preview.page, None);
        assert!(preview.text.iter().any(|segment| segment.hit));
        assert_eq!(preview.marks.len(), 1);
        assert_eq!((preview.marks[0].x0, preview.marks[0].y0), (0.1, 0.25));

        // A word the picture does not contain marks nothing, even though the
        // file's name is still text on the tile.
        let miss = service
            .preview(&path.to_string_lossy(), "zebra", None)
            .unwrap();
        assert!(miss.marks.is_empty());
    }

    // -- status -----------------------------------------------------------

    #[test]
    fn status_names_the_folders_kinds_and_pending_highlights() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let shot = corpus.path().join("samurai.png");
        png(&shot, 800, 400);
        let notes = corpus.path().join("notes.txt");
        std::fs::write(&notes, b"words").unwrap();
        let paper = corpus.path().join("invoice.pdf");
        std::fs::write(&paper, b"not really a pdf").unwrap();
        index_with_tiles(
            &service,
            corpus.path(),
            &shot,
            Kind::Image,
            &[picture_tile("samurai.png", "a figure by the bridge")],
        );
        index_as(&service, corpus.path(), &notes, Kind::Text);
        index_as(&service, corpus.path(), &paper, Kind::Pdf);

        let status = service.status().unwrap();
        assert_eq!(status.version, tessera_core::VERSION);
        assert!(!status.ocr_engine.is_empty());
        assert_eq!(
            status.counts_by_kind,
            vec![
                ("image".to_string(), 1),
                ("pdf".to_string(), 1),
                ("text".to_string(), 1)
            ]
        );
        assert_eq!(status.folder_rows.len(), 1);
        assert_eq!(status.folder_rows[0].count, 3);
        assert_eq!(status.folder_rows[0].path, corpus.path().to_string_lossy());
        assert_eq!(
            status.highlight_pending, 0,
            "fresh rows already carry their line boxes"
        );
    }

    // -- thumbnails -------------------------------------------------------

    fn facts(path: &Path) -> tessera_core::index::skip::FileFacts {
        let meta = std::fs::metadata(path).expect("the fixture exists");
        tessera_core::index::skip::FileFacts {
            size: meta.len(),
            mtime: 1_000,
            placeholder: false,
        }
    }

    /// Put a file in the service's index the way an index run would.
    fn index_as(service: &AppService, root: &Path, path: &Path, kind: Kind) {
        index_with_tiles(service, root, path, kind, &[]);
    }

    fn index_with_tiles(
        service: &AppService,
        root: &Path,
        path: &Path,
        kind: Kind,
        tiles: &[Tile],
    ) {
        let store = Store::open(service.data_dir()).unwrap();
        store
            .index_file(root, path, kind, &facts(path), None, tiles)
            .unwrap();
    }

    fn set_size(service: &AppService, path: &Path, width: u32, height: u32) {
        let store = Store::open(service.data_dir()).unwrap();
        let file_id = store.file_id(path).unwrap();
        store.set_dimensions(file_id, width, height).unwrap();
    }

    /// An image tile whose first line is the filename and whose second line is
    /// a piece of visible text sitting at a known place on the picture.
    fn picture_tile(name: &str, line: &str) -> Tile {
        Tile {
            seq: 0,
            text: format!("{name}\n{line}"),
            page: None,
            boxes: vec![
                None,
                Some(NormBox {
                    x0: 0.1,
                    y0: 0.25,
                    x1: 0.6,
                    y1: 0.4,
                }),
            ],
        }
    }

    fn png(path: &Path, width: u32, height: u32) {
        image::RgbImage::from_pixel(width, height, image::Rgb([200, 120, 60]))
            .save(path)
            .expect("write the fixture");
    }

    #[test]
    fn a_thumbnail_is_only_made_for_an_indexed_image_that_is_still_on_disk() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let image_path = corpus.path().join("shot.png");
        png(&image_path, 40, 20);
        let not_an_image = corpus.path().join("notes.txt");
        std::fs::write(&not_an_image, b"words").unwrap();

        // No index at all is the first-run state, not a miss.
        let error = service
            .thumbnail(&image_path.to_string_lossy())
            .unwrap_err();
        assert!(error.contains("no index yet"), "{error}");

        index_as(&service, corpus.path(), &not_an_image, Kind::Text);
        let error = service
            .thumbnail(&not_an_image.to_string_lossy())
            .unwrap_err();
        assert!(error.contains("only made for image"), "{error}");

        // A path the index has never seen is refused before the filesystem is
        // consulted, so the command cannot read arbitrary files.
        let outside = corpus.path().join("outside.png");
        png(&outside, 10, 10);
        let error = service.thumbnail(&outside.to_string_lossy()).unwrap_err();
        assert!(error.contains("not in the index"), "{error}");

        // A row whose file has been deleted since indexing names the fact.
        let gone = corpus.path().join("gone.png");
        let store = Store::open(temp.path()).unwrap();
        store
            .index_file(
                corpus.path(),
                &gone,
                Kind::Image,
                &tessera_core::index::skip::FileFacts {
                    size: 3,
                    mtime: 1_000,
                    placeholder: false,
                },
                None,
                &[],
            )
            .unwrap();
        drop(store);
        let error = service.thumbnail(&gone.to_string_lossy()).unwrap_err();
        assert!(error.contains("reading"), "{error}");
    }

    #[test]
    fn thumbnail_is_480() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let path = corpus.path().join("shot.png");
        png(&path, 1200, 800);
        index_as(&service, corpus.path(), &path, Kind::Image);
        let path_str = path.to_string_lossy().into_owned();

        let first = service.thumbnail(&path_str).unwrap();
        assert_eq!(service.thumbnail_decodes(), 1);
        assert!(
            first.starts_with(&[0xFF, 0xD8]),
            "a thumbnail is a JPEG, so the list can trust one MIME type"
        );
        let decoded = image::load_from_memory(&first).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (480, 320),
            "the longest edge is 480 and the aspect ratio is kept"
        );

        // The second request answers from the cache: the same bytes, one
        // decode, one file under the cache directory.
        let second = service.thumbnail(&path_str).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            service.thumbnail_decodes(),
            1,
            "the second request re-decoded the picture instead of reading the cache"
        );
        let entries: Vec<_> = std::fs::read_dir(temp.path().join("thumbs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert!(entries[0].ends_with(".jpg"), "{entries:?}");

        // And the index records the entry that now exists, so a future sweep
        // can tell which files still refer to it.
        let store = Store::open(temp.path()).unwrap();
        let source = store.thumb_source(&path).unwrap().unwrap();
        assert_eq!(source.thumb.as_deref(), Some(entries[0].as_str()));

        // The same cap holds for a picture taller than it is wide.
        let tall = corpus.path().join("tall.png");
        png(&tall, 200, 1000);
        index_as(&service, corpus.path(), &tall, Kind::Image);
        let tall_bytes = service.thumbnail(&tall.to_string_lossy()).unwrap();
        let tall_decoded = image::load_from_memory(&tall_bytes).unwrap();
        assert_eq!((tall_decoded.width(), tall_decoded.height()), (96, 480));
        assert_eq!(service.thumbnail_decodes(), 2);
    }

    #[test]
    fn a_corrupt_image_is_an_error_and_never_a_panic() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let path = corpus.path().join("broken.png");
        // A PNG header followed by noise: enough to be claimed as an image,
        // not enough to decode.
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n not really a picture").unwrap();
        index_as(&service, corpus.path(), &path, Kind::Image);
        let path_str = path.to_string_lossy().into_owned();

        let error = service.thumbnail(&path_str).unwrap_err();
        assert!(error.contains("could not be decoded"), "{error}");
        // A second ask is still an error — the failure is not cached as bytes
        // and does not take the process with it.
        assert!(service.thumbnail(&path_str).is_err());
        assert_eq!(service.thumbnail_decodes(), 0);
    }

    #[test]
    fn an_oversized_source_is_refused_before_it_is_read() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let path = corpus.path().join("huge.png");
        let file = std::fs::File::create(&path).unwrap();
        // Sparse on NTFS and ext4, so the test writes no gigabytes.
        file.set_len(MAX_THUMBNAIL_SOURCE_BYTES + 1).unwrap();
        drop(file);
        index_as(&service, corpus.path(), &path, Kind::Image);

        let error = service.thumbnail(&path.to_string_lossy()).unwrap_err();
        assert!(error.contains("larger than"), "{error}");
        assert_eq!(service.thumbnail_decodes(), 0);
    }

    #[test]
    fn removing_a_folder_forgets_its_rows_and_keeps_its_files() {
        let temp = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let shot = first.path().join("shot.png");
        png(&shot, 40, 20);
        let notes = second.path().join("notes.txt");
        std::fs::write(&notes, b"words").unwrap();
        index_as(&service, first.path(), &shot, Kind::Image);
        index_as(&service, second.path(), &notes, Kind::Text);
        assert_eq!(service.status().unwrap().folder_rows.len(), 2);

        let removed = service
            .remove_root(&first.path().to_string_lossy())
            .unwrap();
        assert_eq!(removed, 1);
        assert!(shot.exists(), "the picture on disk is untouched");

        // Only the named folder is forgotten; the other keeps its rows.
        let status = service.status().unwrap();
        assert_eq!(status.folder_rows.len(), 1);
        assert_eq!(status.folder_rows[0].path, second.path().to_string_lossy());
        let pictures = service.library(&["image".to_string()], 10).unwrap();
        assert!(pictures.rows.is_empty());
        assert_eq!(pictures.pictures, 0);
    }

    #[test]
    fn clearing_thumbnails_removes_only_the_cache() {
        let temp = tempfile::tempdir().unwrap();
        let corpus = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let path = corpus.path().join("shot.png");
        png(&path, 120, 80);
        index_as(&service, corpus.path(), &path, Kind::Image);
        service.thumbnail(&path.to_string_lossy()).unwrap();
        let cache = temp.path().join("thumbs");
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);

        assert_eq!(service.clear_thumbnails().unwrap(), 1);
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 0);
        {
            let store = Store::open(temp.path()).unwrap();
            assert!(store.thumb_source(&path).unwrap().unwrap().thumb.is_none());
        }
        assert!(path.exists(), "the picture on disk is untouched");
        assert_eq!(
            service
                .library(&["image".to_string()], 10)
                .unwrap()
                .rows
                .len(),
            1,
            "the index row stays; only its cache entry is gone"
        );
        assert_eq!(service.clear_thumbnails().unwrap(), 0);
    }

    #[test]
    fn the_model_state_is_verified_once_and_then_cached() {
        // The window asks for status on a timer, and verifying the model files
        // hashes gigabytes. One status call may verify; the next must not.
        let temp = tempfile::tempdir().unwrap();
        let service = AppService::new(temp.path(), temp.path().join("models"));
        assert!(!service.models_are_cached());
        let first = service.status().unwrap();
        assert!(service.models_are_cached());
        let second = service.status().unwrap();
        assert_eq!(first.models, second.models);
        assert!(service.models_are_cached());
    }

    #[test]
    fn two_searches_reuse_one_model_load() {
        // A search used to reconstruct both embedders, re-hashing ~1.47 GB of
        // models and rebuilding the ONNX sessions. The second query in one
        // process has to reuse them.
        let temp = tempfile::tempdir().unwrap();
        drop(Store::open(temp.path()).unwrap());
        let service = AppService::new(temp.path(), temp.path().join("models"));
        assert_eq!(service.search_loads(), 0);
        let first = service.search("invoice", SearchMode::All, 10).unwrap();
        assert_eq!(service.search_loads(), 1);
        let second = service.search("invoice", SearchMode::All, 10).unwrap();
        assert_eq!(
            service.search_loads(),
            1,
            "the second query rebuilt the embedders instead of reusing them"
        );
        assert_eq!(first.rows, second.rows);
        assert!(
            second.millis >= second.engine_millis,
            "the total must include the engine time"
        );
    }

    #[test]
    fn installing_models_drops_the_loaded_embedders_too() {
        // The cache describes the files on disk. An install replaces them, so
        // both the status answer and the loaded sessions are stale.
        let temp = tempfile::tempdir().unwrap();
        drop(Store::open(temp.path()).unwrap());
        let service = AppService::new(temp.path(), temp.path().join("models"));
        service.search("invoice", SearchMode::All, 10).unwrap();
        assert!(service.search_embedders_are_cached());
        service.install_models(&[]).unwrap();
        assert!(!service.search_embedders_are_cached());
        service.search("invoice", SearchMode::All, 10).unwrap();
        assert_eq!(service.search_loads(), 2);
    }

    #[test]
    fn search_echoes_the_lens() {
        // The window drops a payload whose mode is not the lens on screen, so
        // the answer has to carry the lens that produced it.
        let temp = tempfile::tempdir().unwrap();
        drop(Store::open(temp.path()).unwrap());
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let payload = service.search("invoice", SearchMode::Pictures, 10).unwrap();
        assert_eq!(payload.mode, "pictures");
        assert!(payload.rows.is_empty());
    }

    #[test]
    fn an_unknown_lens_is_an_error() {
        assert_eq!(parse_lens(None).unwrap(), SearchMode::All);
        assert_eq!(parse_lens(Some("words")).unwrap(), SearchMode::Words);
        let error = parse_lens(Some("videos")).unwrap_err();
        assert!(error.contains("not a lens"), "{error}");
        assert!(error.contains("videos"), "{error}");
    }

    #[test]
    fn pictures_without_the_model_says_so() {
        // The note names the lens's own arm; a pictures query must not be told
        // that the meaning arm is off, because pictures never runs it.
        let temp = tempfile::tempdir().unwrap();
        drop(Store::open(temp.path()).unwrap());
        let service = AppService::new(temp.path(), temp.path().join("models"));
        let payload = service.search("samurai", SearchMode::Pictures, 10).unwrap();
        assert!(
            payload
                .notes
                .iter()
                .any(|note| note.contains("picture search is off")),
            "{:?}",
            payload.notes
        );
        assert!(
            payload.notes.iter().all(|note| !note.contains("meaning")),
            "{:?}",
            payload.notes
        );
    }
}
