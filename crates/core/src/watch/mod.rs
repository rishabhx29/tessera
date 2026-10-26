//! Watch mode: keep the corpus fresh while the machine is in use.
//!
//! `tessera watch` is the lived experience of the product definition's §3.3:
//! a file saved now is searchable in seconds, even while a large backlog is
//! still processing; the queue yields to the person using the machine; and
//! killing the process loses no work that was already recorded.
//!
//! ## The shape
//!
//! One coordinator thread and up to four workers.
//!
//! - The **coordinator** owns everything shared: the queue, the notify
//!   registrations, the power poll, the model lifecycle, and the per-drain
//!   duties (`remove_missing` and the text-arm sweep). Nothing else writes to
//!   the index except through a worker.
//! - Each **worker** owns its own [`Store`] connection (WAL, one writer at a
//!   time, a busy timeout rather than a dropped file) and calls
//!   [`index::process_one`] — the same per-file pass `tessera index` runs, so a
//!   fix there fixes both.
//!
//! ## Why the queue is newest-first
//!
//! A backlog built by an initial scan of 20,000 files is mostly old. If it is
//! processed in walk order, the file the person just saved waits behind all of
//! it, which is the one thing a background indexer cannot do. The backlog is a
//! max-heap on mtime, so a mid-backlog insert is yielded first; dedupe keeps
//! the several events a save produces from becoming several extractions.
//!
//! ## What pauses, and what does not
//!
//! The **queue** pauses — on battery under the default schedule, or because the
//! user chose `paused`. A worker always finishes the file it holds: stopping
//! between extraction and the row would leave work done and nothing recorded.
//!
//! ## What "no events implementation is reliable" means here
//!
//! notify is the trigger, not the source of truth. Every event re-stats the
//! path, a 60 s rescan re-walks each root, and a notify overflow (which drops
//! events by design) is treated as a full rescan. A missed event costs at most
//! one rescan interval.

mod backlog;
mod models;
mod power;
mod priority;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::index::skip::{FileFacts, FileProbe, SystemProbe};
use crate::index::{self, IndexSummary, Kind, Pipeline};
use crate::models::Models;
use crate::ocr::OcrChoice;
use crate::store::{Arm, Store};
use crate::vectors::TextEmbedder;

use backlog::{Backlog, Job};
use models::{Drain, DrainModels};

pub use power::{PowerSource, PowerState, Schedule, SystemPower};

/// How often the power state is re-read. Twice a minute: the transitions that
/// matter are rare, and the call is microseconds.
const POWER_POLL: Duration = Duration::from_secs(30);

/// How often a root is re-walked whether or not an event arrived. The safety
/// net for network shares, for events lost while paused, and for an overflow.
const RESCAN: Duration = Duration::from_secs(60);

/// The longest the idle coordinator sleeps when no timer is nearer.
///
/// This is a *shutdown* ceiling, not a save-latency one: a delivered event wakes
/// `EventFeed::next` immediately, so a file saved onto an idle queue is read as
/// soon as notify delivers it (measured at 10 ms mid-drain, `docs/performance.md`).
/// What the ceiling bounds is the missed-event path and how long a Ctrl-C can go
/// unnoticed, which is why it exists at all. A second is ten times coarser than
/// the old fixed 100 ms tick and still far above the 30 s the nearest real timer
/// (the power poll) needs; the loop parks on whichever comes first.
const IDLE_CAP: Duration = Duration::from_secs(1);

/// While a drain is running the coordinator sleeps on the *result* channel
/// instead, so a batch of files is not paced by the event tick. Five
/// milliseconds is below what a person can notice on a save and high enough
/// that a background process is not waking the CPU a thousand times a second.
const BUSY_TICK: Duration = Duration::from_millis(5);

/// How often "still working" is printed while a drain is long.
const PROGRESS: Duration = Duration::from_secs(5);

/// Tiles embedded per sweep step. Small enough that a fresh file outranks the
/// rest of a large sweep, large enough that the per-page store round trip does
/// not dominate.
const SWEEP_PAGE: usize = 32;

/// The first wait after a coordinator store call failed. A busy database is
/// usually busy for a moment (another index run finishing, a WAL checkpoint), so
/// the first retry is soon; the delay doubles from here.
const COORDINATOR_BACKOFF: Duration = Duration::from_secs(1);

/// Where the coordinator's retry delay stops growing. Past this, a store that
/// keeps refusing is a problem to report, not to wait out.
const COORDINATOR_BACKOFF_CAP: Duration = Duration::from_secs(60);

/// Consecutive failed coordinator calls before the watcher gives up.
///
/// With the delay above this is roughly fifteen minutes of a store that cannot
/// be written: long enough to ride out another process's long transaction,
/// short enough that a permanently broken index is reported instead of a
/// process that looks alive and records nothing.
const MAX_COORDINATOR_FAILURES: u32 = 20;

/// What `tessera watch` was asked to do.
#[derive(Debug, Clone)]
pub struct Options {
    /// The indexed folders to watch. Each queued file records the root it came
    /// from, so deletion stays scoped.
    pub roots: Vec<PathBuf>,
    pub schedule: Schedule,
    /// Worker threads. Clamped to 1..=4 (product-definition §3.3).
    pub workers: usize,
    /// ONNX intra-op threads per session. The embedding sessions are shared by
    /// every worker (one copy of the weights, not four), so this is what one
    /// image embedding gets; 4 keeps a 4-worker drain from oversubscribing an
    /// 8-thread machine.
    pub threads: usize,
    pub rescan: Duration,
    pub power_poll: Duration,
    /// The longest the idle loop sleeps before looking again. Not a poll
    /// interval: the loop parks on the nearest timer and only this ceiling —
    /// the shutdown latency bound — caps it.
    pub tick: Duration,
    pub progress: Duration,
    /// Pause the queue without stopping the watcher. The CLI has no switch for
    /// this (its pause is the power schedule); the desktop shell does, and
    /// putting it on the options keeps one answer to "may a file be dispatched"
    /// instead of a second loop in the UI. `None` means never paused.
    pub pause: Option<Arc<AtomicBool>>,
    /// Lower the whole process to idle priority. True for `tessera watch`, where
    /// the process is the watcher; false for the desktop shell, where the same
    /// process owns the window and demoting it would make the UI yield to
    /// everything else. Worker threads are lowered either way.
    pub idle_priority: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            schedule: Schedule::default(),
            workers: 4,
            threads: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .clamp(1, 4),
            rescan: RESCAN,
            power_poll: POWER_POLL,
            tick: IDLE_CAP,
            progress: PROGRESS,
            pause: None,
            idle_priority: true,
        }
    }
}

impl Options {
    /// Whether the desktop shell has asked the queue to hold. Distinct from the
    /// power schedule on purpose: the user's switch and the machine's power
    /// state are two different reasons for the same answer.
    fn paused_by_app(&self) -> bool {
        self.pause
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }
}

/// What a watch run did before it stopped.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WatchSummary {
    pub indexed: usize,
    pub unchanged: usize,
    pub moved: usize,
    pub skipped: usize,
    pub failed: usize,
    pub awaiting_ocr: usize,
    pub removed: usize,
    pub walk_errors: usize,
    pub unembedded: usize,
    pub text_embedded: usize,
    pub text_unembedded: usize,
    /// Files a worker could not record at all (a store error, not a file the
    /// pipeline refused). Separate from `failed`, which is recorded per file.
    pub errors: usize,
    /// Coordinator store calls that failed and were retried. A run with any of
    /// these recorded fewer files than it was asked to, so the stop line says
    /// so instead of looking clean.
    pub coordinator_errors: usize,
    /// Files a walk found that still need work but whose retry deadline has not
    /// passed. Counted, not queued: "waiting to retry" is a fact about the
    /// corpus, and leaving it out of the banner would make a folder of
    /// unreadable files look empty.
    pub deferred: usize,
}

impl WatchSummary {
    fn absorb(&mut self, summary: &IndexSummary) {
        self.indexed += summary.indexed;
        self.unchanged += summary.unchanged;
        self.moved += summary.moved;
        self.skipped += summary.skipped;
        self.failed += summary.failed;
        self.awaiting_ocr += summary.awaiting_ocr;
        self.removed += summary.removed;
        self.walk_errors += summary.walk_errors;
        self.unembedded += summary.unembedded;
        self.text_embedded += summary.text_embedded;
        self.text_unembedded += summary.text_unembedded;
    }
}

/// Where a run's human-readable lines go.
///
/// A trait so a test can hold the lines instead of printing them: the pause
/// reason and the "caught up" line are acceptance criteria, not side effects.
pub trait Reporter {
    fn line(&mut self, text: &str);
}

/// The real reporter: one line per event, on stdout.
pub struct Console;

impl Reporter for Console {
    fn line(&mut self, text: &str) {
        println!("{text}");
    }
}

/// One filesystem notification batch, in the watcher's own terms.
#[derive(Debug, Clone, Default)]
pub struct WatchEvent {
    pub paths: Vec<PathBuf>,
    /// notify lost events (an overflow, a backend reset) or asked for a
    /// rescan: re-walk the roots instead of trusting the batch.
    pub rescan: bool,
}

/// Where events come from. A trait so the loop can be driven deterministically
/// in a test without depending on how fast a platform delivers a notification.
pub trait EventFeed {
    /// Wait up to `timeout` for one batch. `None` means nothing arrived.
    fn next(&mut self, timeout: Duration) -> Result<Option<WatchEvent>>;
}

/// A monotonic clock, so the rescan and power intervals are testable without
/// sleeping through them.
pub trait Clock: Send + Sync {
    /// Time since the process started: what every interval in the loop counts.
    fn now(&self) -> Duration;

    /// Seconds since the Unix epoch: the unit the store records a row's last
    /// attempt in, so retry deadlines and the store's `indexed_at` are
    /// comparable. A test clock moves both together, which is what lets a
    /// retry be due without sleeping for it.
    fn wall(&self) -> i64;
}

/// The real clock: time since the process started.
pub struct Monotonic {
    start: Instant,
}

impl Monotonic {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for Monotonic {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for Monotonic {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    fn wall(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

/// Start watching: the production entry point.
///
/// `shutdown` is the Ctrl-C flag. The handler is installed by the caller so the
/// CLI owns the terminal behaviour; the loop only reads the flag.
pub fn run(
    data_dir: &Path,
    models: Models,
    ocr: OcrChoice,
    options: Options,
    shutdown: &Arc<AtomicBool>,
) -> Result<WatchSummary> {
    let mut reporter = Console;
    run_with_reporter(data_dir, models, ocr, options, shutdown, &mut reporter)
}

/// Watch with the caller's reporter: the same loop as [`run`], with the lines
/// that describe it going somewhere other than stdout.
///
/// The desktop shell needs the pause reason, the progress lines and the "caught
/// up" line in its own state. Keeping one loop instead of a second one in the
/// UI is what makes the two modes agree about what "indexed" means.
pub fn run_with_reporter(
    data_dir: &Path,
    models: Models,
    ocr: OcrChoice,
    mut options: Options,
    shutdown: &Arc<AtomicBool>,
    reporter: &mut dyn Reporter,
) -> Result<WatchSummary> {
    options.roots = existing_roots(options.roots)?;
    options.workers = options.workers.clamp(1, 4);

    // Before any worker exists, so nothing is spawned at normal priority and
    // then demoted. Non-Windows is a documented no-op.
    if options.idle_priority {
        priority::idle_process_priority().context("putting the watcher at idle priority")?;
    }

    let store = Store::open(data_dir)?;
    forget_stale_text_arm(&store, &models)?;
    // The coordinator opens its own connection below; this one only answered
    // the pin question.
    drop(store);

    let threads = options.threads;
    let models = Box::new(models::EngineModels::new(models, ocr, threads));
    // Register before the initial scan: a file saved while the scan runs then
    // produces an event instead of being missed until the next rescan.
    let mut events = NotifyFeed::new(&options.roots)?;
    let parts = Parts {
        models,
        power: Box::new(SystemPower),
        clock: Box::new(Monotonic::new()),
        events: &mut events,
    };
    run_with(data_dir, options, parts, shutdown.as_ref(), reporter)
}

/// Drop the text arm when the pin moved, before a sweep can mix two vector
/// spaces. The same rule `tessera index` applies, in the same place in the
/// order: this is a property of the index, not of the command that noticed it.
fn forget_stale_text_arm(store: &Store, models: &Models) -> Result<()> {
    for stale in models.stale_arms(store)? {
        if stale.arm == Arm::Text && stale.is_from_a_different_model() {
            store.forget_text_arm()?;
            eprintln!(
                "warning: the MiniLM pin moved (vectors built by {}, pinned is {}); dropping the \
                 text arm's vectors, which this run rebuilds from the text already indexed",
                stale.built_by.as_deref().unwrap_or("an unknown model"),
                stale.pinned
            );
        }
    }
    Ok(())
}

/// Keep the roots that still exist. A folder deleted after it was indexed is a
/// fact about the corpus, not a reason to refuse to watch the others.
fn existing_roots(roots: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let mut kept = Vec::new();
    for root in roots {
        if root.is_dir() {
            kept.push(root);
        } else {
            eprintln!(
                "not watching {}: it is not a folder any more",
                root.display()
            );
        }
    }
    if kept.is_empty() {
        anyhow::bail!(
            "nothing to watch: no indexed folder exists. Index one first: tessera index <folder>"
        );
    }
    Ok(kept)
}

/// The pieces a run is assembled from. Grouped rather than passed one by one so
/// the test seam does not make the production call unreadable.
struct Parts<'a> {
    models: Box<dyn DrainModels>,
    power: Box<dyn PowerSource>,
    clock: Box<dyn Clock>,
    events: &'a mut dyn EventFeed,
}

fn run_with<'a>(
    data_dir: &Path,
    options: Options,
    parts: Parts<'a>,
    shutdown: &'a AtomicBool,
    reporter: &'a mut dyn Reporter,
) -> Result<WatchSummary> {
    let mut watcher = Watcher::open(data_dir, options, parts, shutdown, reporter)?;
    watcher.initial_scan()?;
    while watcher.poll_once()? {}
    watcher.stop();
    Ok(watcher.stats)
}

/// One unit of work sent to a worker.
enum Message {
    File {
        job: Job,
        pipelines: Arc<Vec<Arc<dyn Pipeline>>>,
    },
    Stop,
}

/// A worker thread: its inbox, and the file it is holding (if any).
struct Worker {
    tx: Sender<Message>,
    handle: Option<JoinHandle<()>>,
    busy: Option<PathBuf>,
}

/// What a worker reports back. One file per message, so the coordinator's
/// counters cannot disagree with the store.
struct WorkerResult {
    path: Option<PathBuf>,
    summary: IndexSummary,
    error: Option<String>,
}

/// The text arm's drain-end sweep, in pages.
///
/// A sweep over the store rather than work inside the per-file pass, for the
/// reason `index_folder_with` gives: the repair path and the fresh-install path
/// are the same code. Paged because the watcher shares the machine — a fresh
/// file must be able to outrank the rest of a large sweep.
struct Sweep {
    text: Arc<dyn TextEmbedder>,
    cursor: i64,
    embedded: usize,
    unembedded: usize,
}

impl Sweep {
    /// Embed one page. `true` when the sweep is finished.
    fn step(&mut self, store: &Store) -> Result<bool> {
        let page = store.tiles_without_text_vectors_after(self.cursor, SWEEP_PAGE)?;
        let full = page.len() == SWEEP_PAGE;
        for (tile_id, tile_text) in page {
            self.cursor = self.cursor.max(tile_id);
            match self.text.embed_text(&tile_text) {
                Ok(embedding) => {
                    store.put_text_embedding(tile_id, &embedding)?;
                    self.embedded += 1;
                }
                Err(err) => {
                    // One unembeddable tile must not end the sweep or the run.
                    // It stays in the work list for the next drain, which is
                    // why the arm's pin is not claimed when any failed.
                    eprintln!("could not embed tile {tile_id} for meaning search: {err:#}");
                    self.unembedded += 1;
                }
            }
        }
        Ok(!full)
    }
}

/// The wait decision behind [`Watcher::park`], as a free function so the rule
/// can be tested without a store, a thread and a filesystem.
///
/// `drain_open` is work too: the drain-end sweep is real work the loop must not
/// page at ten steps a second. `next_timer` is how long until the nearest
/// deadline the loop keeps for itself (a rescan, the power poll, a retry); when
/// nothing is owed, the loop sleeps exactly that long, capped at `tick` so a
/// stop request is noticed. Sleeping for the nearest timer instead of a fixed
/// tick is the difference between one wakeup every 30 s and ten a second.
fn park(
    tick: Duration,
    in_flight: bool,
    drain_open: bool,
    queued: bool,
    dispatchable: bool,
    next_timer: Duration,
) -> Duration {
    if in_flight || drain_open {
        return Duration::ZERO;
    }
    if queued && dispatchable {
        return Duration::ZERO;
    }
    next_timer.min(tick)
}

/// How long the coordinator waits after `failures` consecutive store errors.
/// One second, doubling to [`COORDINATOR_BACKOFF_CAP`].
fn coordinator_backoff(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(16);
    (COORDINATOR_BACKOFF * (1 << doublings)).min(COORDINATOR_BACKOFF_CAP)
}

/// How long the coordinator sleeps when a store call failed and its retry
/// deadline has not passed: the rest of the backoff, capped at `tick` so a stop
/// request is still noticed within it.
///
/// Deliberately *not* `next_timer`. That returns `ZERO` for a timer that is
/// already due — the right answer for `park`, which means "do not sleep, this
/// pass has work to do", and the wrong answer here: a pass that is waiting out
/// an outage services no timer at all (the power poll, the rescan), so a zero
/// wait is an immediate return and the loop spins for the whole backoff. A due
/// timer must not be able to shorten this wait.
fn outage_wait(tick: Duration, now: Duration, retry_at: Duration) -> Duration {
    retry_at.saturating_sub(now).min(tick)
}

/// The coordinator.
struct Watcher<'a> {
    options: Options,
    store: Store,
    models: Box<dyn DrainModels>,
    power: Box<dyn PowerSource>,
    clock: Box<dyn Clock>,
    events: &'a mut dyn EventFeed,
    reporter: &'a mut dyn Reporter,
    shutdown: &'a AtomicBool,
    policy: power::PowerPolicy,

    backlog: Backlog,
    /// Paths a worker is holding. A path is never dispatched twice at once: two
    /// writers on one file is duplicate work at best and a lost update at worst.
    in_flight: HashSet<PathBuf>,
    /// Paths that changed *while* they were in flight. Re-admitted when the
    /// worker returns, so the second save is not lost to the first.
    recheck: HashSet<PathBuf>,

    drain: Option<Drain>,
    sweep: Option<Sweep>,
    workers: Vec<Worker>,
    results: Receiver<WorkerResult>,
    /// Set when a worker's channel refuses a file: its thread is gone for good,
    /// so it is dropped and the run ends if it was the last one. Without this a
    /// dead worker would be selected again on every pass and the queue would
    /// spin on a thread that can never take the work.
    out_of_workers: bool,

    last_power: Option<Duration>,
    last_rescan: Duration,
    last_progress: Duration,
    retry_drain_at: Option<Duration>,
    rescan_wanted: bool,
    dirty: bool,
    paused_said: bool,
    /// The app-pause state the last pass saw, so the transition is reported
    /// once and the models are released when the queue goes quiet.
    app_paused_said: bool,

    /// Consecutive coordinator store failures, and the instant the coordinator
    /// may touch the store again. A failed call is retried after a growing
    /// delay, so a transient lock does not become either a dead watcher or a
    /// busy loop against a database that is refusing writes.
    coordinator_failures: u32,
    retry_coordinator_at: Option<Duration>,

    /// Counts since the current drain began, for the "caught up" line.
    drain_counts: IndexSummary,
    stats: WatchSummary,
}

impl<'a> Watcher<'a> {
    fn open(
        data_dir: &Path,
        options: Options,
        parts: Parts<'a>,
        shutdown: &'a AtomicBool,
        reporter: &'a mut dyn Reporter,
    ) -> Result<Self> {
        let store = Store::open(data_dir)?;
        let (result_tx, results) = mpsc::channel();
        let workers = (0..options.workers)
            .map(|index| spawn_worker(data_dir, &format!("tessera-watch-{index}"), &result_tx))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            policy: power::PowerPolicy::new(options.schedule),
            options,
            store,
            models: parts.models,
            power: parts.power,
            clock: parts.clock,
            events: parts.events,
            reporter,
            shutdown,
            backlog: Backlog::new(),
            in_flight: HashSet::new(),
            recheck: HashSet::new(),
            drain: None,
            sweep: None,
            workers,
            results,
            out_of_workers: false,
            last_power: None,
            last_rescan: Duration::ZERO,
            last_progress: Duration::ZERO,
            retry_drain_at: None,
            rescan_wanted: false,
            dirty: true,
            paused_said: false,
            app_paused_said: false,
            coordinator_failures: 0,
            retry_coordinator_at: None,
            drain_counts: IndexSummary::default(),
            stats: WatchSummary::default(),
        })
    }

    fn stopped(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// The whole loop body: handle what arrived, then decide what to do.
    /// `false` means the run is over.
    ///
    /// A coordinator store call that fails is not the end of the run. The store
    /// is shared with `tessera index`, a backup tool and the WAL itself, so a
    /// write can be refused for five seconds and then work again; ending a
    /// watcher meant to run for days on the first such refusal is the wrong
    /// trade (and the model-load and worker paths already back off rather than
    /// die). The failure is reported, a rescan is owed, and the cycle is
    /// retried after a growing delay — until the same store has failed
    /// [`MAX_COORDINATOR_FAILURES`] times in a row, which is not transient.
    fn poll_once(&mut self) -> Result<bool> {
        if self.stopped() {
            return Ok(false);
        }
        if self.out_of_workers {
            self.reporter
                .line("every worker thread stopped; the watch loop is ending");
            return Ok(false);
        }
        if let Some(at) = self.retry_coordinator_at {
            let now = self.clock.now();
            if now < at {
                // Nothing the coordinator could do this pass: a store call
                // failed and its retry deadline has not passed. Sleep until that
                // deadline — or until an event arrives — instead of hammering a
                // store that is refusing writes.
                //
                // The wait is capped by `tick`, never by `next_timer`: a due
                // timer is reported there as `ZERO` ("do not sleep, work now"),
                // and this pass deliberately services none of them, so a zero
                // here would be an immediate return and the loop would spin at
                // full speed for the rest of the backoff. That is what turned a
                // 60 s wait into ~30 s of a burnt core, once per retry.
                let wait = outage_wait(self.options.tick, now, at);
                // An event delivered during the outage is read and dropped on
                // purpose: nothing can be recorded, so queueing it would only
                // grow a list the store could not absorb. The failure already
                // owes a rescan, and every failed pass re-arms it, so a save that
                // lands while the store is refusing is found by the walk that
                // follows recovery.
                let _ = self.events.next(wait);
                return Ok(true);
            }
            self.retry_coordinator_at = None;
        }
        match self.cycle() {
            Ok(()) => self.coordinator_failures = 0,
            Err(err) => self.coordinator_failed(err)?,
        }
        Ok(true)
    }

    /// One pass of the loop's own work. Fallible only through the store.
    fn cycle(&mut self) -> Result<()> {
        self.absorb_event(self.park())?;
        self.sync_app_pause();
        self.poll_power();
        // Before dispatch and settle, not after: a rescan is where a *retry*
        // becomes due, and work queued after `settle` would let one iteration
        // end a drain and print "caught up" while the retry it just queued is
        // still owed. Walking first means the drain ends only when the queue
        // the walk produced is empty too.
        self.rescan_if_due()?;
        self.dispatch();
        let wait_for_results = if self.in_flight.is_empty() {
            None
        } else {
            Some(BUSY_TICK)
        };
        self.collect(wait_for_results)?;
        self.settle()?;
        self.progress_if_due();
        Ok(())
    }

    fn coordinator_failed(&mut self, err: anyhow::Error) -> Result<()> {
        self.coordinator_failures += 1;
        self.stats.coordinator_errors += 1;
        if self.coordinator_failures >= MAX_COORDINATOR_FAILURES {
            return Err(err.context(format!(
                "the index could not be updated {MAX_COORDINATOR_FAILURES} times in a row; \
                 stopping rather than pretending to watch"
            )));
        }
        let delay = coordinator_backoff(self.coordinator_failures);
        self.reporter.line(&format!(
            "the index could not be updated ({err:#}); retrying in {:.1} s",
            delay.as_secs_f64()
        ));
        self.retry_coordinator_at = Some(self.clock.now() + delay);
        // Whatever the failed call would have noticed is still owed: a walk
        // finds additions, and the deletion sweep follows the walk.
        self.dirty = true;
        self.rescan_wanted = true;
        Ok(())
    }

    /// How long the coordinator may block before it has to look again.
    ///
    /// `ZERO` means "block only for something that is already there": a worker
    /// is holding a file, a drain is open with a sweep to finish, or a queued
    /// file can be dispatched right now. Sleeping a full tick for any of those
    /// would pace a four-worker drain at forty files a second.
    ///
    /// Otherwise the loop sleeps until the nearest deadline it keeps for itself
    /// — never longer than [`Options::tick`]. A held queue must sleep, not spin:
    /// a paused watcher burning a core is the exact opposite of the idle
    /// priority this mode exists to promise.
    ///
    /// A timer reported as due (`ZERO`) is safe *here* because the pass that
    /// parks also runs it, in the same iteration. [`outage_wait`] is the one
    /// wait that cannot rely on that.
    fn park(&self) -> Duration {
        park(
            self.options.tick,
            !self.in_flight.is_empty(),
            self.drain.is_some(),
            !self.backlog.is_empty(),
            self.admits() && !self.retry_drain_at.is_some_and(|at| self.clock.now() < at),
            self.next_timer(),
        )
    }

    /// Whether a queued file may go to a worker right now. Two independent
    /// reasons to say no: the power schedule, and the desktop shell's pause
    /// switch. Both leave the file queued and the CPU asleep.
    fn admits(&self) -> bool {
        self.policy.admits() && !self.options.paused_by_app()
    }

    /// How long until the nearest timer the loop keeps for itself.
    ///
    /// `ZERO` means "a timer is due *now*", not "the loop may be idle": the pass
    /// that sees it services the timer in the same iteration. It is therefore a
    /// sleep duration only for a pass that will run those timers — see
    /// [`outage_wait`] for the pass that does not.
    fn next_timer(&self) -> Duration {
        let now = self.clock.now();
        // `None` means "has never run", so it is due now rather than later.
        let since = |at: Option<Duration>, every: Duration| match at {
            Some(at) => (at + every).saturating_sub(now),
            None => Duration::ZERO,
        };
        let mut nearest = since(Some(self.last_rescan), self.options.rescan)
            .min(since(self.last_power, self.options.power_poll));
        for at in [self.retry_drain_at, self.retry_coordinator_at]
            .into_iter()
            .flatten()
        {
            nearest = nearest.min(at.saturating_sub(now));
        }
        nearest
    }

    // -- intake ---------------------------------------------------------

    fn initial_scan(&mut self) -> Result<()> {
        let roots = self.options.roots.clone();
        for root in &roots {
            self.scan_root(root, true)?;
        }
        let deferred = if self.stats.deferred == 0 {
            String::new()
        } else {
            format!(", {} waiting to retry (not due yet)", self.stats.deferred)
        };
        self.reporter.line(&format!(
            "watching {} folder(s); {} file(s) need indexing, {} already up to date{deferred}; \
             process priority: {}",
            roots.len(),
            self.backlog.len(),
            self.stats.unchanged,
            priority::process_priority_name()
        ));
        Ok(())
    }

    fn absorb_event(&mut self, wait: Duration) -> Result<()> {
        let Some(event) = self.events.next(wait)? else {
            return Ok(());
        };
        if event.rescan {
            // notify dropped events or asked for a rescan. The batch cannot be
            // trusted, so the roots are re-walked instead.
            self.rescan_wanted = true;
        }
        for path in event.paths {
            if path.is_dir() {
                // A new directory produces an event for itself; its children
                // arrive separately and the rescan covers anything missed.
                continue;
            }
            let Some(kind) = index::kind_for(&path) else {
                continue;
            };
            let Ok(facts) = SystemProbe.facts(&path) else {
                // Gone, or unreadable: the deletion sweep decides at drain end.
                self.dirty = true;
                continue;
            };
            if !index::needs_work(
                self.store.file_state(&path)?.as_ref(),
                &facts,
                self.clock.wall(),
            ) {
                continue;
            }
            let Some(root) = self.root_for(&path) else {
                // Not under any watched root: not ours to index.
                continue;
            };
            self.enqueue(&root, &path, kind, facts);
        }
        Ok(())
    }

    /// The most specific watched root containing `path`.
    fn root_for(&self, path: &Path) -> Option<PathBuf> {
        self.options
            .roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.as_os_str().len())
            .cloned()
    }

    fn enqueue(&mut self, root: &Path, path: &Path, kind: Kind, facts: FileFacts) {
        if self.in_flight.contains(path) {
            // A save that lands while the file is being processed must not be
            // lost: process it again when the worker returns.
            self.recheck.insert(path.to_path_buf());
            return;
        }
        self.backlog.push(Job {
            root: root.to_path_buf(),
            path: path.to_path_buf(),
            kind,
            facts,
        });
    }

    /// Stat-only walk of one root. Files whose metadata says they are already
    /// indexed are counted, not queued — that is what makes a restart cheap.
    /// A file that still needs work but whose retry is not due is counted as
    /// deferred, so "nothing to do" never hides a folder that is not indexed.
    fn scan_root(&mut self, root: &Path, count_unchanged: bool) -> Result<usize> {
        let now = self.clock.wall();
        let mut queued = 0;
        for entry in WalkDir::new(root).follow_links(false) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    self.stats.walk_errors += 1;
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let Some(kind) = index::kind_for(path) else {
                continue;
            };
            let Ok(facts) = SystemProbe.facts(path) else {
                self.stats.walk_errors += 1;
                continue;
            };
            let state = self.store.file_state(path)?;
            if !index::needs_work(state.as_ref(), &facts, now) {
                // `deferred` is counted by every walk, not only the first: the
                // stop line reads it as "waiting to retry now", and a startup
                // snapshot would be stale after a retry succeeded (or missed a
                // file that started failing later). `unchanged` stays a
                // first-scan figure — it is about what the run found, not what
                // is true at the end.
                if state.is_some_and(|state| state.extracted) {
                    if count_unchanged {
                        self.stats.unchanged += 1;
                    }
                } else {
                    self.stats.deferred += 1;
                }
                continue;
            }
            let root = root.to_path_buf();
            self.enqueue(&root, path, kind, facts);
            queued += 1;
        }
        Ok(queued)
    }

    // -- power ----------------------------------------------------------

    fn poll_power(&mut self) {
        let now = self.clock.now();
        if self
            .last_power
            .is_some_and(|at| now.saturating_sub(at) < self.options.power_poll)
        {
            return;
        }
        self.last_power = Some(now);
        let state = self.power.state();
        if let Some(admits) = self.policy.observe(state) {
            if admits {
                self.paused_said = false;
                self.reporter.line(&format!(
                    "resumed: {} (schedule: {})",
                    describe_power(state),
                    self.policy.schedule()
                ));
            } else {
                self.paused_said = true;
                self.reporter.line(&format!(
                    "paused: {} (schedule: {}); files stay queued",
                    pause_reason(self.policy.schedule(), state),
                    self.policy.schedule()
                ));
                // A pause is exactly when idle RAM matters: let the models go
                // now instead of holding ~250 MB until the backlog resumes.
                self.release_models();
            }
        }
    }

    /// Drop the drain's sessions without ending the drain's bookkeeping.
    fn release_models(&mut self) {
        self.drain = None;
        self.sweep = None;
    }

    /// Report the app's pause switch when it changes, and let the models go on
    /// the way in. The flag is read once here and once in [`Watcher::admits`];
    /// everything that gates dispatch goes through the second.
    ///
    /// A pause is exactly when idle RAM matters (product-definition §3.3), so
    /// the ~250 MB of sessions is dropped immediately rather than held until
    /// the next drain would have ended anyway.
    fn sync_app_pause(&mut self) {
        let paused = self.options.paused_by_app();
        if paused == self.app_paused_said {
            return;
        }
        self.app_paused_said = paused;
        // The two pause sources share one "said it once" flag, so a resume here
        // lets the schedule speak again if it is still holding the queue.
        self.paused_said = paused;
        if paused {
            self.reporter
                .line("paused: the desktop app asked the watcher to pause; files stay queued");
            self.release_models();
        } else {
            self.reporter
                .line("resumed: the desktop app asked the watcher to resume");
        }
    }

    // -- dispatch and results -------------------------------------------

    fn dispatch(&mut self) {
        if self.backlog.is_empty() {
            return;
        }
        if !self.admits() {
            if !self.paused_said {
                self.paused_said = true;
                if self.options.paused_by_app() {
                    self.reporter.line(&format!(
                        "paused: the desktop app asked the watcher to pause; {} file(s) stay queued",
                        self.backlog.len()
                    ));
                } else {
                    self.reporter.line(&format!(
                        "paused: schedule is {}; {} file(s) stay queued",
                        self.policy.schedule(),
                        self.backlog.len()
                    ));
                }
            }
            return;
        }
        if self.retry_drain_at.is_some_and(|at| self.clock.now() < at) {
            return;
        }
        if self.drain.is_none() && !self.load_drain() {
            return;
        }
        let pipelines = match self.drain.as_ref() {
            Some(drain) => Arc::new(drain.pipelines.clone()),
            None => return,
        };
        while let Some(index) = self.workers.iter().position(|worker| worker.busy.is_none()) {
            let Some(job) = self.backlog.pop() else {
                break;
            };
            let path = job.path.clone();
            self.in_flight.insert(path.clone());
            self.workers[index].busy = Some(path.clone());
            let message = Message::File {
                job,
                pipelines: pipelines.clone(),
            };
            if let Err(err) = self.workers[index].tx.send(message) {
                // A refused send means the thread is gone for good: it reported
                // why when it stopped. Put the file back in queue order so it is
                // not lost, drop the worker, and — if it was the last one — end
                // the run. Leaving a dead worker in the list is a queue that
                // spins on a thread which can never take the work.
                self.in_flight.remove(&path);
                if let Message::File { job, .. } = err.0 {
                    self.backlog.push(job);
                }
                self.workers[index].busy = None;
                let mut dead = self.workers.remove(index);
                if let Some(handle) = dead.handle.take() {
                    let _ = handle.join();
                }
                self.reporter.line(&format!(
                    "a worker thread stopped; {} worker(s) left",
                    self.workers.len()
                ));
                if self.workers.is_empty() {
                    self.out_of_workers = true;
                }
                continue;
            }
        }
    }

    /// Build one drain's models, or report why not and back off.
    fn load_drain(&mut self) -> bool {
        match self.models.load() {
            Ok(drain) => {
                self.drain = Some(drain);
                self.retry_drain_at = None;
                self.last_progress = self.clock.now();
                true
            }
            Err(err) => {
                self.reporter.line(&format!(
                    "could not load the models for this batch: {err:#}"
                ));
                // Not a tight retry: a broken model set does not fix itself
                // between two ticks, and re-hashing a gigabyte per tick is the
                // opposite of idle.
                self.retry_drain_at = Some(self.clock.now() + self.options.rescan);
                false
            }
        }
    }

    fn collect(&mut self, wait: Option<Duration>) -> Result<()> {
        let wait = if self.in_flight.is_empty() {
            None
        } else {
            wait
        };
        if let Some(wait) = wait {
            if let Ok(result) = self.results.recv_timeout(wait) {
                self.absorb_result(result)?;
            }
        }
        while let Ok(result) = self.results.try_recv() {
            self.absorb_result(result)?;
        }
        Ok(())
    }

    fn absorb_result(&mut self, result: WorkerResult) -> Result<()> {
        if let Some(path) = result.path.as_ref() {
            self.in_flight.remove(path);
            for worker in &mut self.workers {
                if worker.busy.as_deref() == Some(path.as_path()) {
                    worker.busy = None;
                }
            }
        }
        if let Some(error) = &result.error {
            self.stats.errors += 1;
            self.reporter.line(&format!(
                "could not record {}: {error}",
                result
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "a file".to_string())
            ));
        }
        if let Some(path) = result.path.as_ref() {
            if let Some(verb) = describe(&result.summary) {
                self.reporter
                    .line(&format!("{verb:<12} {}", path.display()));
            }
        }
        self.stats.absorb(&result.summary);
        self.drain_counts.absorb(&result.summary);

        // A change that arrived while the file was in flight is new work.
        if let Some(path) = result.path {
            if self.recheck.remove(&path) {
                self.readmit(&path)?;
            }
        }
        Ok(())
    }

    /// Re-stat a path and queue it if it still needs work.
    fn readmit(&mut self, path: &Path) -> Result<()> {
        let Some(kind) = index::kind_for(path) else {
            return Ok(());
        };
        let Ok(facts) = SystemProbe.facts(path) else {
            self.dirty = true;
            return Ok(());
        };
        if !index::needs_work(
            self.store.file_state(path)?.as_ref(),
            &facts,
            self.clock.wall(),
        ) {
            return Ok(());
        }
        let Some(root) = self.root_for(path) else {
            return Ok(());
        };
        self.enqueue(&root, path, kind, facts);
        Ok(())
    }

    // -- drains ---------------------------------------------------------

    /// End-of-drain duties, run only when nothing is queued and nothing is in
    /// flight. The main loop owns these, so no worker ever races a deletion
    /// sweep against a file it is writing.
    fn settle(&mut self) -> Result<()> {
        if !self.backlog.is_empty() || !self.in_flight.is_empty() {
            return Ok(());
        }
        if self.dirty {
            self.run_removals()?;
            self.dirty = false;
        }
        if self.drain.is_none() {
            return Ok(());
        }
        let has_text = self
            .drain
            .as_ref()
            .is_some_and(|drain| drain.text.is_some());
        if has_text {
            if self.sweep.is_none() {
                self.start_sweep();
            }
            if !self.step_sweep()? {
                return Ok(());
            }
        }
        self.end_drain();
        Ok(())
    }

    fn run_removals(&mut self) -> Result<()> {
        let roots = self.options.roots.clone();
        let mut removed = 0;
        for root in &roots {
            removed += self.store.remove_missing(root)?;
        }
        if removed > 0 {
            self.stats.removed += removed;
            self.reporter.line(&format!(
                "removed {removed} file(s) that are gone from disk"
            ));
        }
        Ok(())
    }

    fn start_sweep(&mut self) {
        let Some(drain) = self.drain.as_ref() else {
            return;
        };
        let Some(text) = drain.text.clone() else {
            return;
        };
        self.sweep = Some(Sweep {
            text,
            cursor: 0,
            embedded: 0,
            unembedded: 0,
        });
    }

    fn step_sweep(&mut self) -> Result<bool> {
        let Some(sweep) = self.sweep.as_mut() else {
            return Ok(true);
        };
        let done = sweep.step(&self.store)?;
        self.drain_counts.text_embedded =
            self.sweep.as_ref().map(|sweep| sweep.embedded).unwrap_or(0);
        self.drain_counts.text_unembedded = self
            .sweep
            .as_ref()
            .map(|sweep| sweep.unembedded)
            .unwrap_or(0);
        Ok(done)
    }

    /// Release the drain's models and say what the drain did.
    fn end_drain(&mut self) {
        let Some(drain) = self.drain.take() else {
            return;
        };
        let sweep = self.sweep.take();
        if let (Some(version), Some(sweep)) = (drain.text_version.as_deref(), sweep.as_ref()) {
            if sweep.unembedded == 0 {
                if let Err(err) = self.store.set_arm_version(Arm::Text, version) {
                    eprintln!("could not record the text arm's model version: {err:#}");
                }
            }
        }
        let counts = self.drain_counts;
        self.drain_counts = IndexSummary::default();
        self.stats.text_embedded += counts.text_embedded;
        self.stats.text_unembedded += counts.text_unembedded;
        self.reporter.line(&format!(
            "caught up: {indexed} indexed, {moved} moved, {skipped} skipped, {failed} failed, \
             {awaiting} awaiting OCR, {embedded} chunk(s) embedded; models released",
            indexed = counts.indexed,
            moved = counts.moved,
            skipped = counts.skipped,
            failed = counts.failed,
            awaiting = counts.awaiting_ocr,
            embedded = counts.text_embedded,
        ));
        // The sessions are dropped here, not at process exit: ~250 MB of model
        // is what "idle" must not hold (product-definition §3.3).
        drop(drain);
    }

    // -- timers ---------------------------------------------------------

    fn rescan_if_due(&mut self) -> Result<()> {
        let now = self.clock.now();
        let due = self.rescan_wanted || now.saturating_sub(self.last_rescan) >= self.options.rescan;
        if !due {
            return Ok(());
        }
        self.rescan_wanted = false;
        self.last_rescan = now;
        let roots = self.options.roots.clone();
        // This walk is the live answer to "how many files are waiting to retry",
        // so start it from zero rather than adding to the last snapshot.
        self.stats.deferred = 0;
        for root in &roots {
            self.scan_root(root, false)?;
        }
        // The walk found additions; it cannot find deletions (a path that is
        // gone has no entry), so the deletion sweep follows the walk.
        self.dirty = true;
        Ok(())
    }

    fn progress_if_due(&mut self) {
        if self.drain.is_none() || self.backlog.is_empty() {
            return;
        }
        let now = self.clock.now();
        if now.saturating_sub(self.last_progress) < self.options.progress {
            return;
        }
        self.last_progress = now;
        self.reporter.line(&format!(
            "working: {} file(s) queued, {} in flight",
            self.backlog.len(),
            self.in_flight.len()
        ));
    }

    // -- shutdown -------------------------------------------------------

    /// Ask every worker to stop and wait for it. A worker finishes the file it
    /// holds: every store write is one transaction, so a stop is never a
    /// half-written row — but stopping *between* the extraction and the row
    /// would be work thrown away, and the wait is what prevents it.
    fn stop(&mut self) {
        for worker in &mut self.workers {
            let _ = worker.tx.send(Message::Stop);
        }
        for worker in &mut self.workers {
            if let Some(handle) = worker.handle.take() {
                let _ = handle.join();
            }
        }
        // A worker finishes the file it holds before it stops, so its result can
        // still be in the channel. Counting it is what makes the "stopped:"
        // line agree with the store: the row was committed either way, and a
        // summary that omits it is a report the user cannot trust.
        while let Ok(result) = self.results.try_recv() {
            let _ = self.absorb_result(result);
        }
        self.workers.clear();
        self.drain = None;
        self.sweep = None;
        self.in_flight.clear();
    }
}

impl Drop for Watcher<'_> {
    fn drop(&mut self) {
        self.stop();
    }
}

fn spawn_worker(data_dir: &Path, name: &str, results: &Sender<WorkerResult>) -> Result<Worker> {
    let (tx, rx) = mpsc::channel::<Message>();
    let results = results.clone();
    let dir = data_dir.to_path_buf();
    let handle = std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || worker_loop(&dir, &rx, &results))
        .context("spawning a watch worker")?;
    Ok(Worker {
        tx,
        handle: Some(handle),
        busy: None,
    })
}

fn worker_loop(dir: &Path, inbox: &Receiver<Message>, results: &Sender<WorkerResult>) {
    if let Err(err) = priority::lowest_thread_priority() {
        eprintln!("could not lower a worker thread's priority: {err:#}");
    }
    let store = match Store::open(dir) {
        Ok(store) => store,
        Err(err) => {
            let _ = results.send(WorkerResult {
                path: None,
                summary: IndexSummary::default(),
                error: Some(format!("opening the index: {err:#}")),
            });
            return;
        }
    };
    while let Ok(message) = inbox.recv() {
        let Message::File { job, pipelines } = message else {
            break;
        };
        let mut summary = IndexSummary::default();
        let error = match pipelines
            .iter()
            .find(|pipeline| pipeline.accepts(&job.path) == Some(job.kind))
        {
            Some(pipeline) => index::process_one(
                &store,
                &job.root,
                &job.path,
                pipeline.as_ref(),
                job.kind,
                &job.facts,
                &mut summary,
            )
            .err()
            .map(|err| format!("{err:#}")),
            None => Some(format!(
                "no pipeline accepts {} any more",
                job.path.display()
            )),
        };
        if results
            .send(WorkerResult {
                path: Some(job.path),
                summary,
                error,
            })
            .is_err()
        {
            // The coordinator is gone; nothing left to report to.
            break;
        }
    }
}

/// The verb for one file's outcome, or `None` for a file that was verified
/// unchanged. `process_one` sets exactly one of these per file.
fn describe(summary: &IndexSummary) -> Option<&'static str> {
    if summary.indexed > 0 {
        Some("indexed")
    } else if summary.moved > 0 {
        Some("moved")
    } else if summary.skipped > 0 {
        Some("skipped")
    } else if summary.failed > 0 {
        Some("failed")
    } else if summary.awaiting_ocr > 0 {
        Some("awaiting OCR")
    } else {
        None
    }
}

fn describe_power(state: PowerState) -> &'static str {
    match state {
        PowerState::Ac => "on AC power",
        PowerState::Battery => "on battery",
        PowerState::Unknown => "power state unknown",
    }
}

fn pause_reason(schedule: Schedule, state: PowerState) -> &'static str {
    match (schedule, state) {
        (Schedule::Paused, _) => "the schedule is paused",
        (_, PowerState::Battery) => "on battery",
        (_, PowerState::Ac) => "paused",
        (_, PowerState::Unknown) => "power state unknown",
    }
}

/// The production event source: notify's recommended backend for the platform.
struct NotifyFeed {
    /// Kept alive for the life of the run: dropping it unregisters everything.
    _watcher: notify::RecommendedWatcher,
    rx: Receiver<notify::Result<notify::Event>>,
}

impl NotifyFeed {
    fn new(roots: &[PathBuf]) -> Result<Self> {
        use notify::Watcher as _;

        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = tx.send(event);
        })
        .context("starting the file watcher")?;
        for root in roots {
            watcher
                .watch(root, notify::RecursiveMode::Recursive)
                .with_context(|| format!("watching {}", root.display()))?;
        }
        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }
}

impl EventFeed for NotifyFeed {
    fn next(&mut self, timeout: Duration) -> Result<Option<WatchEvent>> {
        match self.rx.recv_timeout(timeout) {
            Ok(Ok(event)) => Ok(Some(WatchEvent {
                rescan: event.need_rescan(),
                paths: event.paths,
            })),
            Ok(Err(err)) => {
                // The backend failed. The batch it would have delivered is
                // lost, so the honest response is a full rescan.
                eprintln!("the file watcher reported an error: {err}");
                Ok(Some(WatchEvent {
                    paths: Vec::new(),
                    rescan: true,
                }))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("the file watcher stopped delivering events")
            }
        }
    }
}
