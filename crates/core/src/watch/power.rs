//! Power policy: whether the queue may take new work right now.
//!
//! The product promise is that indexing never competes with the person using
//! the machine: on battery the default schedule pauses intake, and it resumes
//! when the machine is plugged in again. Three schedules, one of which is a
//! pause the user asked for.
//!
//! The source is a trait because a laptop's power state cannot be conjured up
//! in a test, and a policy that is only exercised by unplugging a laptop is a
//! policy nobody has tested. [`SystemPower`] is the one production adapter.
//!
//! What pauses is the **queue**, never a file mid-flight: a worker always
//! finishes the file it has, because stopping between the extraction and the
//! row would leave work done and nothing recorded.

use std::fmt;
use std::str::FromStr;

/// What the machine is running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    Ac,
    Battery,
    /// The platform could not answer. Treated as AC by the idle-only policy:
    /// refusing to index on a machine that simply cannot report its power
    /// state would make watch mode do nothing at all, which is a worse failure
    /// than running when it might be on battery.
    Unknown,
}

/// When indexing is allowed to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Schedule {
    /// Pause intake on battery, resume on AC. The default.
    #[default]
    IdleOnly,
    /// Ignore the power state entirely.
    Always,
    /// Do not index at all until the user changes the schedule.
    Paused,
}

impl Schedule {
    pub fn as_str(self) -> &'static str {
        match self {
            Schedule::IdleOnly => "idle-only",
            Schedule::Always => "always",
            Schedule::Paused => "paused",
        }
    }

    /// Whether this schedule admits work in `state`.
    pub fn admits(self, state: PowerState) -> bool {
        match self {
            Schedule::Paused => false,
            Schedule::Always => true,
            Schedule::IdleOnly => state != PowerState::Battery,
        }
    }
}

impl FromStr for Schedule {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "idle-only" => Ok(Schedule::IdleOnly),
            "always" => Ok(Schedule::Always),
            "paused" => Ok(Schedule::Paused),
            other => Err(format!(
                "{other:?} is not a schedule (idle-only | always | paused)"
            )),
        }
    }
}

impl fmt::Display for Schedule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where the power state comes from. One production adapter; the trait exists
/// so the policy is testable without a battery.
pub trait PowerSource: Send {
    fn state(&self) -> PowerState;
}

/// The operating system's answer.
///
/// Polled every ~30 s rather than subscribed to: the transitions that matter
/// (plugged in, unplugged) are rare, and a poll that costs microseconds twice a
/// minute is not worth a notification API per platform.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemPower;

impl PowerSource for SystemPower {
    fn state(&self) -> PowerState {
        platform_state()
    }
}

#[cfg(windows)]
fn platform_state() -> PowerState {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
        return PowerState::Unknown;
    }
    match status.ACLineStatus {
        // ACLineStatus: 0 offline (on battery), 1 online, 255 unknown.
        0 => PowerState::Battery,
        1 => PowerState::Ac,
        _ => PowerState::Unknown,
    }
}

/// Other platforms have no power adapter yet, so they report what they know:
/// nothing. `Unknown` admits work under the default schedule, which keeps
/// `tessera watch` useful on Linux and macOS — where the battery behaviour is
/// not implemented rather than silently wrong. Stated in `docs/architecture.md`
/// rather than pretended.
#[cfg(not(windows))]
fn platform_state() -> PowerState {
    PowerState::Unknown
}

/// The state machine behind the pause: schedule + last observation in, "may the
/// queue take a file" out, and a transition only when the answer changes.
#[derive(Debug)]
pub struct PowerPolicy {
    schedule: Schedule,
    admits: bool,
}

impl PowerPolicy {
    pub fn new(schedule: Schedule) -> Self {
        // Before the first observation, answer what the schedule says about an
        // unknown state. `Paused` therefore starts paused, which is what the
        // user asked for; the others start admitted and are corrected by the
        // first poll (within one tick).
        Self {
            schedule,
            admits: schedule.admits(PowerState::Unknown),
        }
    }

    pub fn schedule(&self) -> Schedule {
        self.schedule
    }

    /// Fold one observation in. `Some(admits)` when the answer changed, so the
    /// caller can say "paused: on battery" once instead of every tick.
    pub fn observe(&mut self, state: PowerState) -> Option<bool> {
        let admits = self.schedule.admits(state);
        let changed = admits != self.admits;
        self.admits = admits;
        changed.then_some(admits)
    }

    pub fn admits(&self) -> bool {
        self.admits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The state machine over an injected source: the sequence a laptop
    /// actually produces, without a laptop.
    #[test]
    fn idle_only_pauses_on_battery_and_resumes_on_ac() {
        let mut policy = PowerPolicy::new(Schedule::IdleOnly);
        assert!(policy.admits(), "watch starts able to work");
        assert_eq!(policy.observe(PowerState::Ac), None, "still admitted");
        assert_eq!(
            policy.observe(PowerState::Battery),
            Some(false),
            "unplugging pauses the queue"
        );
        assert!(!policy.admits());
        assert_eq!(
            policy.observe(PowerState::Battery),
            None,
            "a second poll must not repeat the announcement"
        );
        assert_eq!(
            policy.observe(PowerState::Ac),
            Some(true),
            "plugging back in resumes it"
        );
        assert!(policy.admits());
    }

    #[test]
    fn always_ignores_the_battery() {
        let mut policy = PowerPolicy::new(Schedule::Always);
        assert_eq!(policy.observe(PowerState::Battery), None);
        assert!(policy.admits(), "the user asked for always");
        assert_eq!(policy.observe(PowerState::Ac), None);
        assert!(policy.admits());
    }

    #[test]
    fn paused_stays_paused_whatever_the_power_does() {
        let mut policy = PowerPolicy::new(Schedule::Paused);
        assert!(!policy.admits(), "paused starts paused, not on first poll");
        assert_eq!(policy.observe(PowerState::Ac), None);
        assert!(!policy.admits());
        assert_eq!(policy.observe(PowerState::Battery), None);
        assert!(!policy.admits());
    }

    #[test]
    fn a_machine_that_cannot_report_is_not_treated_as_a_battery() {
        // Refusing to index because the platform has no answer would make
        // `tessera watch` do nothing on Linux, which is a supported target.
        let mut policy = PowerPolicy::new(Schedule::IdleOnly);
        assert_eq!(policy.observe(PowerState::Unknown), None);
        assert!(policy.admits());
        // But once it reports a battery, the pause is real.
        assert_eq!(policy.observe(PowerState::Battery), Some(false));
    }

    #[test]
    fn the_schedule_names_round_trip_and_an_unknown_one_is_named() {
        for schedule in [Schedule::IdleOnly, Schedule::Always, Schedule::Paused] {
            assert_eq!(Schedule::from_str(schedule.as_str()), Ok(schedule));
            assert_eq!(schedule.to_string(), schedule.as_str());
        }
        let err = Schedule::from_str("sometimes").unwrap_err();
        assert!(err.contains("sometimes"), "{err}");
        assert!(err.contains("idle-only"), "{err}");
    }

    #[test]
    fn the_default_schedule_is_the_one_the_product_definition_names() {
        assert_eq!(Schedule::default(), Schedule::IdleOnly);
    }
}
