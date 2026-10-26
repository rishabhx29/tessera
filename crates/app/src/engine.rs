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

use tessera_core::cli::{image_embedder, text_embedder};
use tessera_core::fusion::{parse_query, search as fused_search};
use tessera_core::index::{index_folder, index_folder_with_text, IndexSummary};
use tessera_core::models::Models;
use tessera_core::ocr::OcrChoice;
use tessera_core::store::Store;
use tessera_core::vectors::{QueryEmbedder, TextEmbedder};

/// One highlighted run of a snippet: `hit` runs are drawn with the match style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub text: String,
    pub hit: bool,
}

/// One result as the list renders it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub kind: String,
    pub page: Option<u32>,
    pub snippet: Vec<Segment>,
    /// `keyword`, `image`, `meaning` — the arms that found this file.
    pub arms: Vec<String>,
}

/// A search answer, including why it may be empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchPayload {
    pub rows: Vec<Row>,
    /// User-facing notes: no index, no models, an arm with nothing to rank.
    pub notes: Vec<String>,
    pub millis: u64,
    pub arms: Vec<String>,
}

/// The preview pane's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub page: Option<u32>,
    /// The text of the matching tile (OCR text for an image), with the query's
    /// terms highlighted.
    pub text: Vec<Segment>,
    /// Present for images: the shell displays this file directly.
    pub image_path: Option<String>,
}

/// Counts and paths for the progress and settings screens.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusPayload {
    pub index_path: String,
    pub index_mb: f64,
    pub folders: Vec<String>,
    /// `(status, count)`, from the store.
    pub counts: Vec<(String, i64)>,
    pub failures: Vec<FailureRow>,
    pub skip_reasons: Vec<(String, i64)>,
    /// `(set, complete, missing files)`.
    pub models: Vec<ModelSetRow>,
    pub image_vectors: i64,
    pub text_vectors: i64,
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
}

impl AppService {
    pub fn new(data_dir: impl Into<PathBuf>, models_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            models_dir: models_dir.into(),
            verified_models: Arc::new(Mutex::new(None)),
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

    /// Run the one search box: every installed arm, fused.
    pub fn search(&self, query: &str, limit: usize) -> Result<SearchPayload, String> {
        if !self.index_exists() {
            return Err("no index yet: add a folder to start indexing".into());
        }
        let parsed = parse_query(&[query.to_string()]).map_err(|err| format!("{err:#}"))?;
        if parsed.terms.is_empty() && parsed.filter.is_empty() {
            return Ok(SearchPayload {
                rows: Vec::new(),
                notes: vec!["Type a word, or a filter such as type:pdf.".into()],
                millis: 0,
                arms: Vec::new(),
            });
        }
        let models = Models::open(&self.models_dir).map_err(|err| format!("{err:#}"))?;
        let image = image_embedder(&models);
        let text = text_embedder(&models);
        let mut notes = Vec::new();
        if image.is_none() {
            notes.push("Image descriptions are off: the siglip models are not installed.".into());
        }
        if text.is_none() {
            notes.push("Meaning search is off: the minilm models are not installed.".into());
        }
        let started = std::time::Instant::now();
        let outcome = fused_search(
            &self.data_dir,
            &parsed,
            image
                .as_deref()
                .map(|embedder| embedder as &dyn QueryEmbedder),
            text.as_ref().map(|embedder| embedder as &dyn TextEmbedder),
            limit.clamp(1, 100),
        )
        .map_err(|err| format!("{err:#}"))?;
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
            arms,
        })
    }

    /// The preview pane: the matched tile's text, highlighted, and the image
    /// path when the hit is an image. A file that has no tile (an image whose
    /// OCR produced nothing) still previews as an image.
    pub fn preview(&self, path: &str, query: &str) -> Result<Preview, String> {
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
        let page = file.tiles.first().and_then(|tile| tile.page);
        let text = file
            .tiles
            .first()
            .map(|tile| highlight(&tile.text, &terms))
            .unwrap_or_default();
        let is_image = file.kind == "image";
        Ok(Preview {
            path: file.path.clone(),
            name: file_name(&file.path),
            kind: file.kind,
            page,
            text,
            image_path: is_image.then_some(file.path),
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

    pub fn status(&self) -> Result<StatusPayload, String> {
        let index_path = self.data_dir.join("index.sqlite");
        let index_mb = std::fs::metadata(&index_path)
            .map(|meta| meta.len() as f64 / 1_048_576.0)
            .unwrap_or(0.0);
        let mut payload = StatusPayload {
            index_path: index_path.display().to_string(),
            index_mb: (index_mb * 10.0).round() / 10.0,
            folders: Vec::new(),
            counts: Vec::new(),
            failures: Vec::new(),
            skip_reasons: Vec::new(),
            models: Vec::new(),
            image_vectors: 0,
            text_vectors: 0,
        };
        if !self.index_exists() {
            payload.models = self.model_rows();
            return Ok(payload);
        }
        let store = Store::open(&self.data_dir).map_err(|err| format!("{err:#}"))?;
        payload.folders = store
            .roots()
            .map_err(|err| format!("{err:#}"))?
            .into_iter()
            .map(|path| path.display().to_string())
            .collect();
        payload.counts = store.stats().map_err(|err| format!("{err:#}"))?;
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
        // The install changed the very files the cached answer describes.
        *self
            .verified_models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        Ok(report.lines())
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
    }
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
        let error = service.search("invoice", 10).unwrap_err();
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
}
