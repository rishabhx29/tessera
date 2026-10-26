//! The watcher as the desktop shell runs it.
//!
//! The same loop as `tessera watch` — one implementation, so "indexed" means the
//! same thing in both — started on a named thread, with the shell's pause flag
//! and a ring of the lines it printed. The shell polls [`WatchHandle::status`]
//! for the progress pane; the counts themselves come from the store through
//! [`crate::engine::AppService::status`], so there is no second counter to drift.
//!
//! Two differences from the CLI entry point:
//!
//! - the process is **not** demoted to idle priority, because the window lives
//!   in this process (worker threads are still lowered);
//! - stopping is a flag plus a join, so the file a worker holds is finished
//!   rather than abandoned.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tessera_core::models::Models;
use tessera_core::ocr::OcrChoice;
use tessera_core::watch::{self, Reporter, WatchSummary};

/// How many reporter lines the shell keeps. Enough for the progress pane to
/// show what happened recently; the store is the durable record.
const LOG_LINES: usize = 200;

/// One watcher run, owned by the shell.
pub struct WatchHandle {
    pause: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    log: Arc<Mutex<VecDeque<String>>>,
    thread: Option<JoinHandle<Result<WatchSummary, String>>>,
}

/// What the progress pane needs to know about the run itself.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WatchStatus {
    pub running: bool,
    pub paused: bool,
    /// Oldest first; the most recent line is last.
    pub log: Vec<String>,
    /// Present after the run has stopped: the summary, or the error that ended
    /// it.
    pub summary: Option<String>,
}

impl WatchHandle {
    /// Start watching `roots`. The models are opened before the thread starts,
    /// so a missing model set is an error the caller can show rather than a
    /// watcher that dies on its first file.
    pub fn start(data_dir: &Path, models_dir: &Path, roots: Vec<PathBuf>) -> Result<Self, String> {
        let models = Models::open(models_dir).map_err(|err| format!("{err:#}"))?;
        let pause = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(AtomicBool::new(false));
        let log = Arc::new(Mutex::new(VecDeque::new()));
        let options = watch::Options {
            roots,
            pause: Some(pause.clone()),
            idle_priority: false,
            ..Default::default()
        };
        let thread = {
            let data_dir = data_dir.to_path_buf();
            let shutdown = shutdown.clone();
            let log = log.clone();
            std::thread::Builder::new()
                .name("tessera-app-watch".into())
                .spawn(move || {
                    let mut reporter = AppReporter(log);
                    watch::run_with_reporter(
                        &data_dir,
                        models,
                        OcrChoice::Native,
                        options,
                        &shutdown,
                        &mut reporter,
                    )
                    .map_err(|err| format!("{err:#}"))
                })
                .map_err(|err| format!("starting the watcher: {err}"))?
        };
        Ok(Self {
            pause,
            shutdown,
            log,
            thread: Some(thread),
        })
    }

    pub fn running(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    pub fn paused(&self) -> bool {
        self.pause.load(Ordering::Relaxed)
    }

    /// Hold or release the queue. Releasing is harmless when already running.
    pub fn set_paused(&self, paused: bool) {
        self.pause.store(paused, Ordering::Relaxed);
    }

    pub fn status(&mut self) -> WatchStatus {
        let mut summary = None;
        if self
            .thread
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
        {
            // Take the handle at most once; a finished watcher cannot be asked
            // for its summary twice.
            if let Some(handle) = self.thread.take() {
                summary = Some(match handle.join() {
                    Ok(Ok(summary)) => format!(
                        "stopped: {} indexed, {} unchanged, {} skipped, {} failed, \
                         {} awaiting OCR",
                        summary.indexed,
                        summary.unchanged,
                        summary.skipped,
                        summary.failed,
                        summary.awaiting_ocr
                    ),
                    Ok(Err(err)) => format!("stopped with an error: {err}"),
                    Err(_) => "stopped: the watcher thread panicked".to_string(),
                });
            }
        }
        WatchStatus {
            running: self.thread.is_some(),
            paused: self.paused(),
            log: self
                .log
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .cloned()
                .collect(),
            summary,
        }
    }

    /// Ask the watcher to stop and wait for it. A worker finishes the file it
    /// holds; the store is one transaction per file, so this cannot leave a
    /// half-written row.
    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Appends a line to the ring, dropping the oldest when full.
fn push_line(log: &mut VecDeque<String>, text: &str) {
    if log.len() == LOG_LINES {
        log.pop_front();
    }
    log.push_back(text.to_string());
}

struct AppReporter(Arc<Mutex<VecDeque<String>>>);

impl Reporter for AppReporter {
    fn line(&mut self, text: &str) {
        let mut log = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        push_line(&mut log, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_ring_drops_the_oldest_line_and_keeps_the_order() {
        let mut log = VecDeque::new();
        for index in 0..(LOG_LINES + 5) {
            push_line(&mut log, &format!("line {index}"));
        }
        assert_eq!(log.len(), LOG_LINES);
        assert_eq!(log.front().unwrap(), "line 5");
        assert_eq!(log.back().unwrap(), &format!("line {}", LOG_LINES + 4));
    }

    #[test]
    fn a_handle_that_was_never_started_reports_a_disabled_queue() {
        let handle = WatchHandle {
            pause: Arc::new(AtomicBool::new(false)),
            shutdown: Arc::new(AtomicBool::new(false)),
            log: Arc::new(Mutex::new(VecDeque::new())),
            thread: None,
        };
        assert!(!handle.running());
        assert!(!handle.paused());
    }
}
