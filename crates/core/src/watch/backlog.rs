//! The watcher's queue: newest first, one live entry per path.
//!
//! Newest-first is the whole ordering policy (product-definition §3.3): a file
//! saved while a deep backlog is processing outranks everything already
//! queued, because the thing a person expects from a background indexer is
//! that the file they just saved is findable, not that the queue is fair.
//!
//! Dedupe is the other half. A save produces several events — a create, a
//! modify, sometimes a metadata-only touch — and enqueuing the same path four
//! times would extract it four times. One live entry per path, with a
//! generation token: re-enqueuing a path that is already queued replaces its
//! position (its mtime may have moved), and the older heap entry is discarded
//! when it surfaces.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::path::PathBuf;

use crate::index::skip::FileFacts;
use crate::index::Kind;

/// One file the watcher owes work on, with everything the per-file pass needs
/// besides the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Job {
    /// The indexed folder this file belongs to — what scopes deletion.
    pub root: PathBuf,
    pub path: PathBuf,
    pub kind: Kind,
    /// The metadata read when the file was admitted. `process_one` takes the
    /// same snapshot the folder walk does, so a file that changes between
    /// admission and processing is caught by the next event or the rescan
    /// rather than half-seen.
    pub facts: FileFacts,
}

/// One heap entry. `job` is the payload; only `mtime` and `path` decide order.
#[derive(Debug)]
struct Entry {
    mtime: i64,
    path: PathBuf,
    token: u64,
    job: Job,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    /// Greatest first, because this is a max-heap: newest `mtime` pops first.
    /// Ties break by path **ascending** (`other.path.cmp(&self.path)`), which
    /// makes the order total and reproducible instead of dependent on which
    /// event the filesystem happened to deliver first.
    fn cmp(&self, other: &Self) -> Ordering {
        self.mtime
            .cmp(&other.mtime)
            .then_with(|| other.path.cmp(&self.path))
    }
}

/// A newest-first queue with one live entry per path.
#[derive(Debug, Default)]
pub(crate) struct Backlog {
    heap: BinaryHeap<Entry>,
    /// Path → the token of the entry that is still live. A heap entry whose
    /// token no longer matches is stale and is skipped when it surfaces.
    live: HashMap<PathBuf, u64>,
    next_token: u64,
}

impl Backlog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `job`, or move an existing entry for the same path to its new
    /// position. The path is queued once no matter how many events name it.
    pub fn push(&mut self, job: Job) {
        self.next_token += 1;
        let token = self.next_token;
        self.live.insert(job.path.clone(), token);
        self.heap.push(Entry {
            mtime: job.facts.mtime,
            path: job.path.clone(),
            token,
            job,
        });
    }

    /// The newest job, discarding entries a later `push` superseded.
    pub fn pop(&mut self) -> Option<Job> {
        while let Some(entry) = self.heap.pop() {
            if self.live.get(&entry.path) == Some(&entry.token) {
                self.live.remove(&entry.path);
                return Some(entry.job);
            }
        }
        None
    }

    pub fn len(&self) -> usize {
        self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(name: &str, mtime: i64) -> Job {
        Job {
            root: PathBuf::from("/corpus"),
            path: PathBuf::from("/corpus").join(name),
            kind: Kind::Text,
            facts: FileFacts {
                size: 1,
                mtime,
                placeholder: false,
            },
        }
    }

    fn names(backlog: &mut Backlog) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(job) = backlog.pop() {
            out.push(job.path.file_name().unwrap().to_string_lossy().into_owned());
        }
        out
    }

    #[test]
    fn the_newest_file_is_yielded_first() {
        let mut backlog = Backlog::new();
        backlog.push(job("old.txt", 100));
        backlog.push(job("newest.txt", 300));
        backlog.push(job("middle.txt", 200));
        assert_eq!(names(&mut backlog), ["newest.txt", "middle.txt", "old.txt"]);
    }

    #[test]
    fn a_file_saved_mid_backlog_is_yielded_first() {
        // The acceptance criterion: the newest 100 files are searchable while
        // older files are still processing. A file inserted after the queue was
        // built must not wait behind it.
        let mut backlog = Backlog::new();
        for i in 0..100 {
            backlog.push(job(&format!("old-{i}.txt"), i));
        }
        backlog.push(job("just-saved.txt", 10_000));
        assert_eq!(backlog.len(), 101);
        assert_eq!(
            backlog.pop().unwrap().path,
            PathBuf::from("/corpus/just-saved.txt")
        );
        assert_eq!(
            backlog.pop().unwrap().path,
            PathBuf::from("/corpus/old-99.txt"),
            "the rest keep their newest-first order"
        );
    }

    #[test]
    fn a_path_queued_twice_is_processed_once() {
        // A save produces a create and a modify. Two extractions of the same
        // bytes is work the user paid for twice.
        let mut backlog = Backlog::new();
        backlog.push(job("a.txt", 100));
        backlog.push(job("a.txt", 100));
        backlog.push(job("a.txt", 100));
        assert_eq!(backlog.len(), 1, "three events, one entry");
        assert_eq!(names(&mut backlog), ["a.txt"]);
        assert!(backlog.is_empty());
    }

    #[test]
    fn re_enqueuing_moves_the_entry_to_its_new_mtime() {
        let mut backlog = Backlog::new();
        backlog.push(job("a.txt", 100));
        backlog.push(job("b.txt", 200));
        // `a.txt` is saved again: its mtime moves past `b.txt`.
        backlog.push(job("a.txt", 300));
        assert_eq!(backlog.len(), 2, "still one entry for a.txt");
        assert_eq!(names(&mut backlog), ["a.txt", "b.txt"]);
    }

    #[test]
    fn a_path_can_be_queued_again_after_it_was_processed() {
        let mut backlog = Backlog::new();
        backlog.push(job("a.txt", 100));
        assert_eq!(backlog.pop().unwrap().path, PathBuf::from("/corpus/a.txt"));
        backlog.push(job("a.txt", 200));
        assert_eq!(
            backlog.len(),
            1,
            "a second save is new work, not a duplicate"
        );
    }

    #[test]
    fn equal_timestamps_break_by_path_so_the_order_is_total() {
        let mut backlog = Backlog::new();
        backlog.push(job("b.txt", 100));
        backlog.push(job("a.txt", 100));
        assert_eq!(
            names(&mut backlog),
            ["a.txt", "b.txt"],
            "a tie must not be an arbitrary order"
        );
    }
}
