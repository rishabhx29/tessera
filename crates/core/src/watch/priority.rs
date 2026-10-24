//! Background process priority.
//!
//! On Windows a background indexer competes with whatever the person is doing
//! through the scheduler like any other process. `IDLE_PRIORITY_CLASS` is the
//! class Windows reserves for work that should yield: it is what Task Manager's
//! "low" priority sets, and it is what makes a foreground build or a video call
//! keep the machine. Worker threads additionally ask for the lowest thread
//! priority, which matters because the process class sets a ceiling, not a
//! value.
//!
//! Other platforms keep their normal priority. That is a stated limitation
//! rather than a silent one: `nice` exists on Unix, but nothing in this build
//! has measured what the right value is there, and a number invented to fill a
//! table is worse than "unchanged".

#[cfg(windows)]
mod imp {
    use anyhow::{Context, Result};
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThread, GetPriorityClass, SetPriorityClass, SetThreadPriority,
        IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, THREAD_PRIORITY_LOWEST,
    };

    /// Put this process in the idle class. Called once, by the watcher, before
    /// any worker starts.
    pub fn idle_process_priority() -> Result<()> {
        // SAFETY: both handles are pseudo-handles for the calling process and
        // thread, valid for the life of the process and owned by nobody.
        unsafe { SetPriorityClass(GetCurrentProcess(), IDLE_PRIORITY_CLASS) }
            .context("setting the process to idle priority")
    }

    /// The lowest thread priority inside that class. Called on each worker
    /// thread, because thread priority is per thread.
    pub fn lowest_thread_priority() -> Result<()> {
        // SAFETY: as above — a pseudo-handle for the calling thread.
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST) }
            .context("setting a worker thread to the lowest priority")
    }

    /// The class this process is actually in, for the startup line and for a
    /// test that reads it from outside.
    pub fn process_priority_name() -> &'static str {
        // SAFETY: a pseudo-handle for the calling process.
        let class = unsafe { GetPriorityClass(GetCurrentProcess()) };
        if class == 0 {
            "unknown"
        } else if class == IDLE_PRIORITY_CLASS.0 {
            "idle"
        } else if class == NORMAL_PRIORITY_CLASS.0 {
            "normal"
        } else {
            "other"
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use anyhow::Result;

    /// No-op: this platform's priority behaviour is unchanged (module docs).
    pub fn idle_process_priority() -> Result<()> {
        Ok(())
    }

    /// No-op, for the same reason.
    pub fn lowest_thread_priority() -> Result<()> {
        Ok(())
    }

    pub fn process_priority_name() -> &'static str {
        "unchanged"
    }
}

pub use imp::{idle_process_priority, lowest_thread_priority, process_priority_name};
