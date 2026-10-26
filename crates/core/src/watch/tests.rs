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

    /// How many lines contain `needle`. A drain's "caught up" line is printed
    /// once per drain, so counting it is how a test says "a second drain ran" —
    /// `contains` cannot, because the first one is still in the log.
    fn count(&self, needle: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|line| line.contains(needle))
            .count()
    }

    /// How many times `verb` was the outcome for a path whose file name is
    /// `name`. The reporter pads the verb into a column, so matching the whole
    /// line is not an option.
    fn outcomes(&self, verb: &str, name: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|line| line.starts_with(verb) && line.contains(name))
            .count()
    }

    fn wait_for(&self, needle: &str) -> bool {
        wait_until(|| self.contains(needle))
    }

    fn wait_for_count(&self, needle: &str, want: usize) -> bool {
        wait_until(|| self.count(needle) >= want)
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

/// A feed that remembers how long every wait asked for.
///
/// The loop's wait *is* its CPU cost: a pass that asks the feed for five
/// milliseconds sleeps, and a pass that asks for nothing returns at once. That
/// is how a test can tell a sleeping watcher from a spinning one without
/// measuring a core, and it is the difference the sustained-outage test pins.
struct RecordingFeed {
    rx: Receiver<WatchEvent>,
    waits: Arc<Mutex<Vec<Duration>>>,
}

impl RecordingFeed {
    fn new(rx: Receiver<WatchEvent>) -> (Self, Arc<Mutex<Vec<Duration>>>) {
        let waits = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                rx,
                waits: waits.clone(),
            },
            waits,
        )
    }
}

impl EventFeed for RecordingFeed {
    fn next(&mut self, timeout: Duration) -> Result<Option<WatchEvent>> {
        self.waits.lock().unwrap().push(timeout);
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
    /// Where this pipeline notes each path it started on, when a test wants to
    /// pin dispatch order rather than just completion.
    order: Option<Arc<Mutex<Vec<String>>>>,
}

impl Pipeline for GatedPipeline {
    fn accepts(&self, path: &Path) -> Option<Kind> {
        crate::index::text::TextPipeline.accepts(path)
    }

    fn process(&self, path: &Path) -> Result<crate::index::Extraction> {
        if let Some(order) = &self.order {
            order
                .lock()
                .unwrap()
                .push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
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
    order: Option<Arc<Mutex<Vec<String>>>>,
}

impl FakeModels {
    fn new(gate: Arc<Gate>, loads: Arc<AtomicUsize>) -> Self {
        Self {
            gate,
            loads,
            order: None,
        }
    }

    fn recording(mut self, order: Arc<Mutex<Vec<String>>>) -> Self {
        self.order = Some(order);
        self
    }
}

impl DrainModels for FakeModels {
    fn load(&self) -> Result<Drain> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(Drain {
            pipelines: vec![Arc::new(GatedPipeline {
                gate: self.gate.clone(),
                order: self.order.clone(),
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

    fn wall(&self) -> i64 {
        // A row written by a real run carries a real timestamp, which is *after*
        // this instant, so a frozen clock means "no retry is due" rather than
        // "every retry is due". Tests that need a deadline to pass move
        // `TestClock` instead.
        0
    }
}

/// A clock a test moves by hand, for both halves of the loop's sense of time:
/// the intervals behind the rescan and the power poll, and the epoch seconds a
/// retry deadline is compared against. Advancing it is how a test says "ten
/// minutes have passed" without sleeping.
#[derive(Clone)]
struct TestClock {
    secs: Arc<Mutex<u64>>,
    wall_base: i64,
}

impl TestClock {
    fn new() -> Self {
        let wall_base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Self {
            secs: Arc::new(Mutex::new(0)),
            wall_base,
        }
    }

    fn advance(&self, by: Duration) {
        *self.secs.lock().unwrap() += by.as_secs();
    }
}

impl Clock for TestClock {
    fn now(&self) -> Duration {
        Duration::from_secs(*self.secs.lock().unwrap())
    }

    fn wall(&self) -> i64 {
        self.wall_base + *self.secs.lock().unwrap() as i64
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
fn a_held_queue_parks_on_the_nearest_timer_instead_of_spinning_a_core() {
    // The wait decision: a pause, or a drain that failed to load, must not turn
    // into a busy loop; idle time is spent asleep until a timer or a save.
    let cap = Duration::from_secs(1);
    let timer = Duration::from_secs(30);
    // Work the loop must not sleep through: a worker's result, an open drain
    // with a sweep to finish, and a file that can be dispatched right now.
    assert_eq!(park(cap, true, false, true, true, timer), Duration::ZERO);
    assert_eq!(park(cap, false, true, false, true, timer), Duration::ZERO);
    assert_eq!(park(cap, false, false, true, true, timer), Duration::ZERO);
    // Nothing owed: sleep until the nearest timer, which is far past the cap.
    assert_eq!(park(cap, false, false, false, true, timer), cap);
    // A nearer timer wins: this is the "one wakeup per timer, not ten a second"
    // rule the old fixed 100 ms tick did not have.
    assert_eq!(
        park(cap, false, false, false, true, Duration::from_millis(40)),
        Duration::from_millis(40)
    );
    // Nothing owed and no timer yet (nothing has run): the cap bounds it.
    assert_eq!(park(cap, false, false, false, true, Duration::MAX), cap);
    // Something owed but not dispatchable — paused, or backing off a drain that
    // would not load. The queue stays; the CPU rests.
    assert_eq!(park(cap, false, false, true, false, timer), cap);
}

#[test]
fn an_outage_wait_is_never_shortened_by_a_timer_that_is_already_due() {
    // D2 regression: the wait that covers a store outage used the same
    // "nearest timer" answer as `park`, where a *due* timer means "do not
    // sleep, work now". The waiting pass services no timer, so that answer made
    // the wait zero and the loop spun for the rest of the backoff. Here a due
    // power poll (the loop's own timer, deliberately not run while waiting)
    // must not be able to shorten the wait at all.
    let cap = Duration::from_secs(1);
    let deadline = Duration::from_secs(60);
    assert_eq!(outage_wait(cap, Duration::ZERO, deadline), cap);
    assert_eq!(
        outage_wait(cap, Duration::from_secs(30), deadline),
        cap,
        "the second half of a capped backoff, where the power poll has come due"
    );
    // A deadline nearer than the tick is slept to exactly, not overslept.
    assert_eq!(
        outage_wait(cap, Duration::ZERO, Duration::from_millis(100)),
        Duration::from_millis(100)
    );
    // A deadline that has passed is no wait at all: the caller runs the cycle.
    assert_eq!(outage_wait(cap, deadline, deadline), Duration::ZERO);
    assert_eq!(
        outage_wait(cap, deadline + Duration::from_secs(5), deadline),
        Duration::ZERO
    );
}

#[test]
fn a_coordinator_store_error_backs_off_and_gives_up_only_after_many() {
    // One refused write must not end a watcher meant to run for days, and a
    // store that refuses twenty in a row is not transient.
    assert_eq!(coordinator_backoff(1), Duration::from_secs(1));
    assert_eq!(coordinator_backoff(2), Duration::from_secs(2));
    assert_eq!(coordinator_backoff(3), Duration::from_secs(4));
    assert_eq!(coordinator_backoff(6), Duration::from_secs(32));
    assert_eq!(coordinator_backoff(7), COORDINATOR_BACKOFF_CAP);
    assert_eq!(coordinator_backoff(1_000), COORDINATOR_BACKOFF_CAP);
    const {
        assert!(
            MAX_COORDINATOR_FAILURES >= 5,
            "four transient failures in a row must not end the run"
        )
    };
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
                    models: Box::new(FakeModels::new(gate, loads)),
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
                    models: Box::new(FakeModels::new(Gate::new(), loads)),
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
                    models: Box::new(FakeModels::new(Gate::new(), Arc::new(AtomicUsize::new(0)))),
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

#[test]
fn one_worker_takes_the_newest_file_first_and_a_fresh_save_before_the_backlog() {
    // D4: the ordering claim, pinned at the loop's dispatch rather than only in
    // the heap's unit test. With a single worker there is no spare thread to
    // hide a regression: whatever is popped first is processed first. The names
    // are deliberately the reverse of the mtimes, so path order cannot explain
    // the result.
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let old = corpus.path().join("slow-a.txt");
    let new = corpus.path().join("slow-z.txt");
    std::fs::write(&old, "slow backlog file, written first").unwrap();
    std::thread::sleep(Duration::from_millis(30));
    std::fs::write(&new, "slow backlog file, written second").unwrap();
    assert!(
        std::fs::metadata(&new).unwrap().modified().unwrap()
            > std::fs::metadata(&old).unwrap().modified().unwrap(),
        "the two files must differ in mtime or this pins nothing"
    );

    let gate = Gate::new();
    let loads = Arc::new(AtomicUsize::new(0));
    let order = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::default();
    let shutdown = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();

    let run = {
        let corpus = corpus.path().to_path_buf();
        let gate = gate.clone();
        let loads = loads.clone();
        let order = order.clone();
        let recorder = recorder.clone();
        let shutdown = shutdown.clone();
        std::thread::spawn(move || {
            let mut feed = ManualFeed { rx };
            let mut reporter = recorder;
            run_with(
                data.path(),
                options(&corpus, 1),
                Parts {
                    models: Box::new(FakeModels::new(gate, loads).recording(order)),
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
        wait_until(|| gate.blocked() == 1),
        "the single worker never took a backlog file: {:?}",
        recorder.lines()
    );
    assert_eq!(
        order.lock().unwrap().as_slice(),
        ["slow-z.txt"],
        "the newest backlog file must be dispatched first, whatever its name"
    );

    // A save that lands while the only worker is busy: when that worker
    // finishes, the fresh file is the next thing taken — not the older backlog.
    let fresh = corpus.path().join("m-fresh.txt");
    std::fs::write(&fresh, "the invoice from the airline").unwrap();
    tx.send(WatchEvent {
        paths: vec![fresh.clone()],
        rescan: false,
    })
    .unwrap();
    gate.release();

    assert!(
        recorder.wait_for_count("caught up", 1),
        "the drain never completed: {:?}",
        recorder.lines()
    );
    assert_eq!(
        order.lock().unwrap().as_slice(),
        ["slow-z.txt", "m-fresh.txt", "slow-a.txt"],
        "dispatch order is newest-first, and a fresh save outranks the older backlog"
    );

    shutdown.store(true, Ordering::SeqCst);
    run.join().unwrap().unwrap();
}

#[test]
fn a_broken_file_is_retried_on_a_backoff_not_on_every_rescan() {
    // D1: the rescan-rework reproduction. Four files, one unparseable, and a
    // rescan every 60 s. The old rule treated "never extracted" as pending
    // forever, so every walk re-queued the broken row and the process loaded
    // the models again once a minute until it was killed. Here the walk still
    // runs (a file nobody announced is found by it) and the broken row is left
    // alone until its backoff is due.
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    for index in 0..3 {
        std::fs::write(
            corpus.path().join(format!("good-{index}.txt")),
            format!("document {index} about the invoice"),
        )
        .unwrap();
    }
    let broken = corpus.path().join("broken.txt");
    std::fs::write(&broken, [0xFF, 0xFE, 0x00, 0xD8]).unwrap();

    let clock = TestClock::new();
    let loads = Arc::new(AtomicUsize::new(0));
    let recorder = Recorder::default();
    let shutdown = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let data_dir = data.path().to_path_buf();

    let run = {
        let corpus = corpus.path().to_path_buf();
        let clock = clock.clone();
        let loads = loads.clone();
        let recorder = recorder.clone();
        let shutdown = shutdown.clone();
        let data_dir = data_dir.clone();
        std::thread::spawn(move || {
            let mut feed = ManualFeed { rx };
            let mut reporter = recorder;
            let mut watch_options = options(&corpus, 1);
            watch_options.rescan = Duration::from_secs(60);
            run_with(
                &data_dir,
                watch_options,
                Parts {
                    models: Box::new(FakeModels::new(Gate::new(), loads)),
                    power: Box::new(FixedPower(PowerState::Ac)),
                    clock: Box::new(clock),
                    events: &mut feed,
                },
                &shutdown,
                &mut reporter,
            )
        })
    };

    // Drain 1: every file gets exactly one attempt, three succeed, one fails.
    assert!(
        recorder.wait_for_count("caught up", 1),
        "the first drain never finished: {:?}",
        recorder.lines()
    );
    let attempts = |path: &Path| Store::open(&data_dir).unwrap().file_state(path).unwrap();
    let first = attempts(&broken).expect("the broken file is recorded");
    assert!(!first.extracted);
    assert_eq!(first.attempts, 1, "one attempt, not one per file touched");
    assert_eq!(loads.load(Ordering::SeqCst), 1);
    assert_eq!(recorder.outcomes("failed", "broken.txt"), 1);
    assert_eq!(recorder.outcomes("indexed", "good-"), 3);

    // One rescan later. The walk has to run — `late.txt` was never announced —
    // and it must not re-queue the broken row, so no second drain and no
    // second model load for it.
    std::fs::write(corpus.path().join("late.txt"), "written with no event").unwrap();
    clock.advance(Duration::from_secs(61));
    assert!(
        recorder.wait_for_count("caught up", 2),
        "the rescan did not find the unannounced file: {:?}",
        recorder.lines()
    );
    assert_eq!(recorder.outcomes("indexed", "late.txt"), 1);
    assert_eq!(
        recorder.outcomes("failed", "broken.txt"),
        1,
        "the rescan re-queued the broken file: {:?}",
        recorder.lines()
    );
    assert_eq!(attempts(&broken).unwrap().attempts, 1);
    assert_eq!(
        loads.load(Ordering::SeqCst),
        2,
        "a second drain ran for a file that did not need one"
    );

    // Past the first retry deadline (five minutes after the attempt): the row
    // becomes pending work again, and only then.
    clock.advance(Duration::from_secs(4 * 60 + 30));
    assert!(
        recorder.wait_for_count("caught up", 3),
        "a never-extracted row was never retried: {:?}",
        recorder.lines()
    );
    assert_eq!(
        recorder.outcomes("failed", "broken.txt"),
        2,
        "the retry is the second attempt and no more"
    );
    assert_eq!(attempts(&broken).unwrap().attempts, 2);
    assert_eq!(
        recorder.outcomes("indexed", "good-"),
        3,
        "the retry re-extracted files that were already indexed"
    );

    // The second attempt's backoff doubles: one rescan interval later is not
    // enough, so another unannounced file is indexed with no retry alongside it.
    std::fs::write(corpus.path().join("later.txt"), "written with no event").unwrap();
    clock.advance(Duration::from_secs(61));
    assert!(
        recorder.wait_for_count("caught up", 4),
        "the third rescan did not run: {:?}",
        recorder.lines()
    );
    assert_eq!(recorder.outcomes("indexed", "later.txt"), 1);
    assert_eq!(
        recorder.outcomes("failed", "broken.txt"),
        2,
        "the backoff did not double: the broken file was retried on the next rescan"
    );
    assert_eq!(attempts(&broken).unwrap().attempts, 2);

    let _ = tx;
    shutdown.store(true, Ordering::SeqCst);
    run.join().unwrap().unwrap();
}

#[test]
fn a_restart_reports_a_row_that_is_waiting_for_its_retry() {
    // The banner's third clause: a row that still needs work but is not due is
    // not "up to date", and a folder of files nobody can read must not look
    // empty at startup.
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    std::fs::write(corpus.path().join("good.txt"), "the invoice").unwrap();
    std::fs::write(corpus.path().join("broken.txt"), [0xFF, 0xFE, 0x00, 0xD8]).unwrap();
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
    let (_tx, rx) = mpsc::channel();
    let run = {
        let corpus = corpus.path().to_path_buf();
        let recorder = recorder.clone();
        let shutdown = shutdown.clone();
        std::thread::spawn(move || {
            let mut feed = ManualFeed { rx };
            let mut reporter = recorder;
            run_with(
                data.path(),
                options(&corpus, 1),
                Parts {
                    models: Box::new(FakeModels::new(Gate::new(), Arc::new(AtomicUsize::new(0)))),
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
        recorder.wait_for(
            "0 file(s) need indexing, 1 already up to date, 1 waiting to retry (not due yet)"
        ),
        "the banner did not account for the row that is waiting to retry: {:?}",
        recorder.lines()
    );
    shutdown.store(true, Ordering::SeqCst);
    run.join().unwrap().unwrap();
}

#[test]
fn a_transient_store_error_backs_off_instead_of_ending_the_run() {
    // D2: the store is shared with `tessera index`, a backup tool and the WAL
    // itself, so a write can be refused for a few seconds and then work again.
    // The failure is injected from outside the process by renaming the table
    // the coordinator reads — a real SQLite error — and taken away again.
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let path = corpus.path().join("survivor.txt");
    let data_dir = data.path().to_path_buf();

    let recorder = Recorder::default();
    let shutdown = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();

    let run = {
        let corpus = corpus.path().to_path_buf();
        let recorder = recorder.clone();
        let shutdown = shutdown.clone();
        std::thread::spawn(move || {
            let mut feed = ManualFeed { rx };
            let mut reporter = recorder;
            run_with(
                data.path(),
                options(&corpus, 1),
                Parts {
                    models: Box::new(FakeModels::new(Gate::new(), Arc::new(AtomicUsize::new(0)))),
                    power: Box::new(FixedPower(PowerState::Ac)),
                    // A real clock: the backoff is real time, and the point is
                    // that the loop survives a wait it did not choose.
                    clock: Box::new(Monotonic::new()),
                    events: &mut feed,
                },
                &shutdown,
                &mut reporter,
            )
        })
    };
    assert!(recorder.wait_for("0 file(s) need indexing"));

    // The file appears, and the store cannot answer for it.
    std::fs::write(&path, "the invoice from the airline").unwrap();
    let intruder = {
        use rusqlite::Connection;
        let conn = Connection::open(data_dir.join("index.sqlite")).unwrap();
        conn.busy_timeout(Duration::from_secs(5)).unwrap();
        conn
    };
    intruder
        .execute_batch("ALTER TABLE files RENAME TO files_hidden")
        .unwrap();
    tx.send(WatchEvent {
        paths: vec![path.clone()],
        rescan: false,
    })
    .unwrap();

    assert!(
        recorder.wait_for("could not be updated"),
        "one refused store call ended or silently skipped the cycle: {:?}",
        recorder.lines()
    );
    assert!(
        recorder.contains("retrying in 1.0 s"),
        "the failure was not backed off: {:?}",
        recorder.lines()
    );
    assert!(
        !run.is_finished(),
        "the watcher gave up on the first transient store error"
    );

    // The refusal was transient: put the table back and the run records the
    // file, which is the whole point of not exiting.
    intruder
        .execute_batch("ALTER TABLE files_hidden RENAME TO files")
        .unwrap();
    assert!(
        wait_until(|| recorder.outcomes("indexed", "survivor.txt") >= 1),
        "the watcher never recovered from a transient store error: {:?}",
        recorder.lines()
    );
    assert_eq!(
        recorder.count("could not be updated"),
        1,
        "one injected failure, one report"
    );

    shutdown.store(true, Ordering::SeqCst);
    run.join().unwrap().unwrap();
}

/// The wait the last `could not be updated` line announced, in seconds.
fn announced_delay(recorder: &Recorder) -> f64 {
    let line = recorder
        .lines()
        .into_iter()
        .rev()
        .find(|line| line.contains("retrying in "))
        .expect("a failure line with a delay");
    line.rsplit("retrying in ")
        .next()
        .unwrap()
        .trim_end_matches(" s")
        .parse()
        .expect("the announced delay is a number of seconds")
}

#[test]
fn a_sustained_store_outage_sleeps_instead_of_spinning_a_core() {
    // The regression the QA probe found: phase 1 (a three-second outage) cost
    // nothing, phase 2 (a sustained one, with work queued) burnt 44% of a core
    // for as long as it ran while the log showed one line a minute. The
    // mechanism is the *outage wait*, not the retry count: it was
    // `min(retry deadline, next_timer)`, and `next_timer` reports an already due
    // timer — here the power poll, which the waiting pass never runs — as
    // `ZERO`. A zero wait is an immediate return, so the loop spun at millions
    // of passes a second for the rest of the backoff, and the second half of
    // every capped backoff was a burnt core.
    //
    // This pins the wait, not just survival: the whole outage runs on a test
    // clock, and the feed records what the loop asked for. A sleeping loop asks
    // for its tick and gets it; a spinning one asks for nothing, so the pass
    // count explodes in the same real time.
    let data = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let data_dir = data.path().to_path_buf();
    let clock = TestClock::new();
    let loads = Arc::new(AtomicUsize::new(0));
    let recorder = Recorder::default();
    let shutdown = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let (feed, waits) = RecordingFeed::new(rx);

    let run = {
        let corpus = corpus.path().to_path_buf();
        let clock = clock.clone();
        let loads = loads.clone();
        let recorder = recorder.clone();
        let shutdown = shutdown.clone();
        let data_dir = data_dir.clone();
        std::thread::spawn(move || {
            let mut feed = feed;
            let mut reporter = recorder;
            let mut watch_options = options(&corpus, 1);
            // Thirty seconds is the production power poll, and the timer that
            // came due inside a capped backoff in the real run.
            watch_options.power_poll = Duration::from_secs(30);
            run_with(
                &data_dir,
                watch_options,
                Parts {
                    models: Box::new(FakeModels::new(Gate::new(), loads)),
                    power: Box::new(FixedPower(PowerState::Ac)),
                    clock: Box::new(clock),
                    events: &mut feed,
                },
                &shutdown,
                &mut reporter,
            )
        })
    };

    assert!(recorder.wait_for("0 file(s) need indexing"));
    let quiet = waits.lock().unwrap().len();

    // The store breaks from outside the process, exactly as the QA probe broke
    // it: a second connection renames the table every coordinator call reads.
    let intruder = {
        use rusqlite::Connection;
        let conn = Connection::open(data_dir.join("index.sqlite")).unwrap();
        conn.busy_timeout(Duration::from_secs(5)).unwrap();
        conn
    };
    intruder
        .execute_batch("ALTER TABLE files RENAME TO files_hidden")
        .unwrap();

    // Work queued behind the failure: the event cannot be admitted while the
    // store refuses, so the rescan the failure owes is what recovers it.
    let queued = corpus.path().join("queued-during-outage.txt");
    std::fs::write(&queued, "the invoice written while the store was gone").unwrap();
    tx.send(WatchEvent {
        paths: vec![queued.clone()],
        rescan: false,
    })
    .unwrap();
    assert!(
        recorder.wait_for("could not be updated"),
        "the broken store was never noticed: {:?}",
        recorder.lines()
    );

    // Walk the backoff to its cap. Each announced delay is what the loop is
    // waiting for, so the clock moves past it — twice the delay plus a second,
    // so an advance cannot land in the instant between the line being printed
    // and the deadline being set from the same clock. One jump crosses one
    // deadline, so the recorded sequence is exactly the delays the loop chose.
    let mut delays = vec![announced_delay(&recorder)];
    while *delays.last().unwrap() < COORDINATOR_BACKOFF_CAP.as_secs_f64() {
        let announced = *delays.last().unwrap();
        clock.advance(Duration::from_secs(announced as u64 * 2 + 1));
        assert!(
            recorder.wait_for_count("could not be updated", delays.len() + 1),
            "the backoff stopped retrying: {:?}",
            recorder.lines()
        );
        delays.push(announced_delay(&recorder));
    }
    assert_eq!(
        delays,
        vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 60.0],
        "a sustained outage must still back off to the cap, not give up early"
    );

    // A capped backoff is sixty seconds and the power poll is thirty, so the
    // second half of the wait is where the old code spun: the poll had come due
    // and the waiting pass never ran it.
    let before_sample = waits.lock().unwrap().len();
    clock.advance(Duration::from_secs(30));
    std::thread::sleep(Duration::from_millis(150));
    let sample = waits.lock().unwrap()[before_sample..].to_vec();
    assert!(
        !sample.is_empty(),
        "the loop stopped looking at the event feed during the outage; it is \
         not waiting for anything"
    );
    assert!(
        sample.iter().all(|wait| !wait.is_zero()),
        "a zero wait while the retry deadline is still in the future: the \
         waiting pass asked the feed not to block, which is a spin. Waits: {:?}",
        &sample[..sample.len().min(20)]
    );
    assert!(
        sample.len() < 1_000,
        "the loop passed over the outage {} times in 150 ms of a 5 ms tick: it \
         is spinning a core instead of sleeping until the retry",
        sample.len()
    );

    // A save that lands while the store is refusing is absorbed by the waiting
    // pass on purpose: nothing can be recorded, so queueing it would only grow a
    // list the store cannot absorb. Every failure re-arms the rescan, so it must
    // not be lost.
    let during = corpus.path().join("saved-during-outage.txt");
    std::fs::write(&during, "another invoice, written during the outage").unwrap();
    tx.send(WatchEvent {
        paths: vec![during.clone()],
        rescan: false,
    })
    .unwrap();
    // Let the waiting pass take it before the store comes back.
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !run.is_finished(),
        "the watcher gave up during a capped outage"
    );

    // Recovery: the table returns, the retry deadline passes, and the owed walk
    // finds both files — the announced one and the one saved mid-outage.
    intruder
        .execute_batch("ALTER TABLE files_hidden RENAME TO files")
        .unwrap();
    clock.advance(Duration::from_secs(61));
    assert!(
        wait_until(|| {
            recorder.outcomes("indexed", "queued-during-outage.txt") >= 1
                && recorder.outcomes("indexed", "saved-during-outage.txt") >= 1
        }),
        "work queued during the outage was lost: {:?}",
        recorder.lines()
    );
    assert_eq!(
        loads.load(Ordering::SeqCst),
        1,
        "recovering one outage must cost one model load, not one per retry"
    );
    let outage_passes = waits.lock().unwrap().len() - quiet;
    assert!(
        outage_passes < 5_000,
        "the whole outage cost {outage_passes} coordinator passes; a sleeping \
         loop makes a few hundred"
    );

    shutdown.store(true, Ordering::SeqCst);
    run.join().unwrap().unwrap();
}
