//! The watch loop's own tests: real threads, a real store, a fake pipeline.
//!
//! The platform is replaced at the three seams the loop has — the event feed,
//! the power source and the clock — so a test can say exactly when a file was
//! saved and exactly what the machine is running on, instead of sleeping and
//! hoping. Everything below the seams is the production loop.

use super::*;
use std::sync::atomic::AtomicUsize;
use std::sync::{Condvar, Mutex};

/// Holds every line a run printed, so a test can assert on the diagnostics
/// rather than on stdout.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<String>>>);

impl Recorder {
    fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    fn contains(&self, needle: &str) -> bool {
        self.0
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains(needle))
    }

    fn wait_for(&self, needle: &str) -> bool {
        wait_until(|| self.contains(needle))
    }
}

impl Reporter for Recorder {
    fn line(&mut self, text: &str) {
        self.0.lock().unwrap().push(text.to_string());
    }
}

/// An event feed the test drives: the loop's handling is under test, not the
/// platform's notification latency.
struct ManualFeed {
    rx: Receiver<WatchEvent>,
}

impl EventFeed for ManualFeed {
    fn next(&mut self, timeout: Duration) -> Result<Option<WatchEvent>> {
        match self.rx.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(None),
        }
    }
}

/// Blocks the files whose names contain "slow" until the test releases it: a
/// stand-in for a 950 ms OCR of a deep backlog.
struct Gate {
    state: Mutex<(usize, bool)>,
    changed: Condvar,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new((0, false)),
            changed: Condvar::new(),
        })
    }

    fn hold(&self) {
        let mut state = self.state.lock().unwrap();
        state.0 += 1;
        while !state.1 {
            state = self.changed.wait(state).unwrap();
        }
        state.0 -= 1;
    }

    fn blocked(&self) -> usize {
        self.state.lock().unwrap().0
    }

    fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}

struct GatedPipeline {
    gate: Arc<Gate>,
}

impl Pipeline for GatedPipeline {
    fn accepts(&self, path: &Path) -> Option<Kind> {
        crate::index::text::TextPipeline.accepts(path)
    }

    fn process(&self, path: &Path) -> Result<crate::index::Extraction> {
        if path.to_string_lossy().contains("slow") {
            self.gate.hold();
        }
        crate::index::text::TextPipeline.process(path)
    }
}

/// A model set with no models: one gated text pipeline per drain, and a count
/// of how many drains asked for one.
struct FakeModels {
    gate: Arc<Gate>,
    loads: Arc<AtomicUsize>,
}

impl DrainModels for FakeModels {
    fn load(&self) -> Result<Drain> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(Drain {
            pipelines: vec![Arc::new(GatedPipeline {
                gate: self.gate.clone(),
            })],
            text: None,
            text_version: None,
        })
    }
}

struct FixedPower(PowerState);

impl PowerSource for FixedPower {
    fn state(&self) -> PowerState {
        self.0
    }
}

struct FrozenClock;

impl Clock for FrozenClock {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    condition()
}

fn options(corpus: &Path, workers: usize) -> Options {
    Options {
        roots: vec![corpus.to_path_buf()],
        schedule: Schedule::Always,
        workers,
        threads: 1,
        rescan: Duration::from_secs(3600),
        power_poll: Duration::from_secs(3600),
        tick: Duration::from_millis(5),
        progress: Duration::from_secs(3600),
    }
}

#[test]
fn a_held_queue_waits_a_tick_instead_of_spinning_a_core() {
    // The wait decision: a pause, or a drain that failed to load, must not turn
    // into a busy loop. Ten wakeups a second is idle; a core at 100% is not.
    let tick = Duration::from_millis(100);
    // Work the loop must not sleep through: a worker's result, an open drain
    // with a sweep to finish, and a file that can be dispatched right now.
    assert_eq!(park(tick, true, false, true, true), Duration::ZERO);
    assert_eq!(park(tick, false, true, false, true), Duration::ZERO);
    assert_eq!(park(tick, false, false, true, true), Duration::ZERO);
    // Nothing owed: wait for a save.
    assert_eq!(park(tick, false, false, false, true), tick);
    // Something owed but not dispatchable — paused, or backing off a drain that
    // would not load. The queue stays; the CPU rests.
    assert_eq!(park(tick, false, false, true, false), tick);
}

#[test]
fn a_fresh_file_completes_while_a_deep_backlog_is_still_processing() {
    // The acceptance criterion this ticket exists for: a file saved while a
    // backlog is running becomes searchable without waiting for the backlog.
    // Three files are held inside the pipeline; a fourth, saved afterwards,
    // must still finish — taken by the one worker the backlog left free.
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    for index in 0..3 {
        std::fs::write(
            corpus.path().join(format!("slow-{index}.txt")),
            format!("slow backlog file {index}"),
        )
        .unwrap();
    }

    let gate = Gate::new();
    let loads = Arc::new(AtomicUsize::new(0));
    let recorder = Recorder::default();
    let shutdown = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();

    let run = {
        let corpus = corpus.path().to_path_buf();
        let gate = gate.clone();
        let loads = loads.clone();
        let recorder = recorder.clone();
        let shutdown = shutdown.clone();
        std::thread::spawn(move || {
            let mut feed = ManualFeed { rx };
            let mut reporter = recorder;
            run_with(
                data.path(),
                options(&corpus, 4),
                Parts {
                    models: Box::new(FakeModels { gate, loads }),
                    power: Box::new(FixedPower(PowerState::Ac)),
                    clock: Box::new(FrozenClock),
                    events: &mut feed,
                },
                &shutdown,
                &mut reporter,
            )
        })
    };

    assert!(
        wait_until(|| gate.blocked() == 3),
        "the three slow files were never taken by workers: {:?}",
        recorder.lines()
    );
    assert!(
        recorder.contains("3 file(s) need indexing"),
        "the initial scan did not queue the backlog: {:?}",
        recorder.lines()
    );

    let fresh = corpus.path().join("fresh.txt");
    std::fs::write(&fresh, "the invoice from the airline").unwrap();
    tx.send(WatchEvent {
        paths: vec![fresh.clone()],
        rescan: false,
    })
    .unwrap();

    assert!(
        recorder.wait_for("fresh.txt"),
        "a file saved mid-backlog never completed: {:?}",
        recorder.lines()
    );
    assert_eq!(
        gate.blocked(),
        3,
        "the backlog finished too early to prove anything"
    );

    gate.release();
    assert!(
        recorder.wait_for("caught up"),
        "the drain never completed after the gate opened: {:?}",
        recorder.lines()
    );
    assert_eq!(
        recorder
            .lines()
            .iter()
            .filter(|line| line.starts_with("indexed"))
            .count(),
        4,
        "every file should have been indexed exactly once: {:?}",
        recorder.lines()
    );

    // The models are per drain, not per process: a second drain rebuilds them,
    // which is what makes the idle-RAM promise true.
    assert_eq!(loads.load(Ordering::SeqCst), 1, "one drain, one load");
    let later = corpus.path().join("later.txt");
    std::fs::write(&later, "saved after the drain").unwrap();
    tx.send(WatchEvent {
        paths: vec![later],
        rescan: false,
    })
    .unwrap();
    assert!(
        wait_until(|| loads.load(Ordering::SeqCst) == 2),
        "the second drain reused the first drain's models"
    );

    shutdown.store(true, Ordering::SeqCst);
    run.join().unwrap().unwrap();
}

#[test]
fn a_restart_reports_files_unchanged_instead_of_indexing_them_again() {
    // "Kill it and start again" is the resume path, and it must be cheap: every
    // file is re-statted, none is re-extracted, and no model is loaded to find
    // that out.
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    for index in 0..3 {
        std::fs::write(
            corpus.path().join(format!("file-{index}.txt")),
            format!("document {index}"),
        )
        .unwrap();
    }
    let store = Store::open(data.path()).unwrap();
    index::index_folder(
        &store,
        &[Box::new(crate::index::text::TextPipeline)],
        corpus.path(),
    )
    .unwrap();
    drop(store);

    let loads = Arc::new(AtomicUsize::new(0));
    let recorder = Recorder::default();
    let shutdown = Arc::new(AtomicBool::new(false));
    let (_tx, rx) = mpsc::channel();

    let run = {
        let corpus = corpus.path().to_path_buf();
        let loads = loads.clone();
        let recorder = recorder.clone();
        let shutdown = shutdown.clone();
        std::thread::spawn(move || {
            let mut feed = ManualFeed { rx };
            let mut reporter = recorder;
            run_with(
                data.path(),
                options(&corpus, 2),
                Parts {
                    models: Box::new(FakeModels {
                        gate: Gate::new(),
                        loads,
                    }),
                    power: Box::new(FixedPower(PowerState::Ac)),
                    clock: Box::new(FrozenClock),
                    events: &mut feed,
                },
                &shutdown,
                &mut reporter,
            )
        })
    };

    assert!(
        recorder.wait_for("0 file(s) need indexing, 3 already up to date"),
        "a restart did not recognise the indexed files: {:?}",
        recorder.lines()
    );
    assert_eq!(
        loads.load(Ordering::SeqCst),
        0,
        "verifying three unchanged files must not load a model"
    );
    shutdown.store(true, Ordering::SeqCst);
    run.join().unwrap().unwrap();
}

#[test]
fn a_deletion_event_removes_the_row_without_a_backlog() {
    // A delete produces no work to queue, so the deletion sweep has to run
    // because an event said something changed — not because a drain happened to
    // finish. Otherwise a deleted file stays in results until the next save.
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let path = corpus.path().join("gone.txt");
    std::fs::write(&path, "the invoice").unwrap();
    let store = Store::open(data.path()).unwrap();
    index::index_folder(
        &store,
        &[Box::new(crate::index::text::TextPipeline)],
        corpus.path(),
    )
    .unwrap();
    drop(store);

    let recorder = Recorder::default();
    let shutdown = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let data_dir = data.path().to_path_buf();

    let run = {
        let corpus = corpus.path().to_path_buf();
        let recorder = recorder.clone();
        let shutdown = shutdown.clone();
        let data_dir = data_dir.clone();
        std::thread::spawn(move || {
            let mut feed = ManualFeed { rx };
            let mut reporter = recorder;
            run_with(
                &data_dir,
                options(&corpus, 2),
                Parts {
                    models: Box::new(FakeModels {
                        gate: Gate::new(),
                        loads: Arc::new(AtomicUsize::new(0)),
                    }),
                    power: Box::new(FixedPower(PowerState::Ac)),
                    clock: Box::new(FrozenClock),
                    events: &mut feed,
                },
                &shutdown,
                &mut reporter,
            )
        })
    };

    assert!(recorder.wait_for("0 file(s) need indexing"));
    std::fs::remove_file(&path).unwrap();
    tx.send(WatchEvent {
        paths: vec![path.clone()],
        rescan: false,
    })
    .unwrap();
    assert!(
        recorder.wait_for("removed 1 file(s)"),
        "a deleted file stayed in the index: {:?}",
        recorder.lines()
    );
    let store = Store::open(&data_dir).unwrap();
    assert!(
        store.file_state(&path).unwrap().is_none(),
        "the row for a deleted file is still there"
    );

    shutdown.store(true, Ordering::SeqCst);
    run.join().unwrap().unwrap();
}
