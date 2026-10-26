//! Ticket 11 acceptance, through the real binary.
//!
//! The unit tests in `crates/core/src/watch` drive the loop through its seams.
//! These run `tessera watch` as a process, because the two claims that cannot
//! be tested from inside are exactly the ones about the process: what priority
//! class Windows sees it in, and what a restart says after the process was
//! killed rather than asked to stop.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// A running `tessera watch`, with its stdout lines on a channel so a test can
/// wait for one without blocking forever.
struct Watch {
    child: Child,
    lines: Receiver<String>,
}

impl Watch {
    fn spawn(data_dir: &Path, models_dir: &Path, folder: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tessera"))
            .arg("--data-dir")
            .arg(data_dir)
            .arg("--models-dir")
            .arg(models_dir)
            .arg("watch")
            // `always`, not the default `idle-only`: this harness asserts that a
            // drain finishes, and the default pauses on battery. A test whose
            // result depends on the host's power source is not hermetic; the
            // priority test above is the one place the default would matter, and
            // it reads the class, not the schedule.
            .arg("--schedule")
            .arg("always")
            .arg(folder)
            .stdout(Stdio::piped())
            // Drained, not read: a full stderr pipe would block the child.
            .stderr(Stdio::null())
            .spawn()
            .expect("the tessera binary should start");
        let stdout = child.stdout.take().expect("stdout is piped");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }

    /// The first line containing `needle`, or `None` after `timeout`.
    fn wait_for_line(&self, needle: &str, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            match self.lines.recv_timeout(left) {
                Ok(line) if line.contains(needle) => return Some(line),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }

    fn wait_for(&self, needle: &str, timeout: Duration) -> bool {
        self.wait_for_line(needle, timeout).is_some()
    }

    /// Kill the process the way a crash or Task Manager does: no shutdown
    /// signal, no chance to finish a file.
    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write_corpus(dir: &Path, files: usize) {
    for index in 0..files {
        std::fs::write(
            dir.join(format!("doc-{index:03}.txt")),
            format!("document number {index}, about the invoice from the airline"),
        )
        .expect("the test corpus should be writable");
    }
}

/// `(need, up to date)` from the startup line.
fn parse_banner(line: &str) -> Option<(usize, usize)> {
    let before = line.split("file(s) need indexing").next()?;
    let need = before.rsplit(';').next()?.trim().parse().ok()?;
    let up = line
        .split("already up to date")
        .next()?
        .rsplit(',')
        .next()?
        .trim()
        .parse()
        .ok()?;
    Some((need, up))
}

#[cfg(windows)]
mod priority {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        GetPriorityClass, OpenProcess, IDLE_PRIORITY_CLASS, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// The priority class of another process, read the way Task Manager reads
    /// it — from outside, so the child cannot simply claim to be idle.
    pub fn class_of(pid: u32) -> Option<u32> {
        // SAFETY: the handle is opened for the one query below, checked, and
        // closed before returning.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let class = GetPriorityClass(handle);
            let _ = CloseHandle(handle);
            (class != 0).then_some(class)
        }
    }

    pub fn idle() -> u32 {
        IDLE_PRIORITY_CLASS.0
    }
}

#[cfg(windows)]
#[test]
fn the_watcher_runs_in_windows_idle_priority() {
    let data = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    write_corpus(corpus.path(), 1);

    let watch = Watch::spawn(data.path(), models.path(), corpus.path());
    assert!(
        watch.wait_for("watching", Duration::from_secs(60)),
        "the watcher never printed its startup line"
    );
    let class = priority::class_of(watch.child.id())
        .expect("the watcher's priority class should be readable");
    assert_eq!(
        class,
        priority::idle(),
        "the watcher is not in IDLE_PRIORITY_CLASS (0x40); it will compete with the foreground"
    );
}

#[test]
fn killing_the_watcher_mid_drain_and_restarting_does_not_re_extract() {
    // The resume path: kill without warning, restart, and the files that were
    // already recorded must be reported unchanged rather than indexed again.
    let data = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let corpus = tempfile::tempdir().unwrap();
    let total = 400;
    write_corpus(corpus.path(), total);

    {
        let mut first = Watch::spawn(data.path(), models.path(), corpus.path());
        assert!(
            first.wait_for("file(s) need indexing", Duration::from_secs(60)),
            "the first run never started"
        );
        // Killed immediately after the scan: some files may be done, most are
        // not, which is the state the resume has to handle.
        first.kill();
    }

    let second = Watch::spawn(data.path(), models.path(), corpus.path());
    let banner = second
        .wait_for_line("file(s) need indexing", Duration::from_secs(60))
        .expect("the second run never printed its startup line");
    let (need, up_to_date) =
        parse_banner(&banner).unwrap_or_else(|| panic!("unparseable startup line: {banner}"));
    assert_eq!(
        need + up_to_date,
        total,
        "every file must be either queued or reported unchanged, never both and never neither: \
         {banner}"
    );

    // And a run that is allowed to finish reports the whole corpus unchanged on
    // the next start: nothing is re-extracted to find that out.
    assert!(
        second.wait_for("caught up", Duration::from_secs(120)),
        "the resumed run never drained its backlog"
    );
    drop(second);

    let third = Watch::spawn(data.path(), models.path(), corpus.path());
    let banner = third
        .wait_for_line("file(s) need indexing", Duration::from_secs(60))
        .expect("the third run never printed its startup line");
    let (need, up_to_date) =
        parse_banner(&banner).unwrap_or_else(|| panic!("unparseable startup line: {banner}"));
    assert_eq!(
        (need, up_to_date),
        (0, total),
        "a restart re-indexed files that were already indexed: {banner}"
    );
}
