//! Injected time — the engine's
//! decision logic never calls `Utc::now()` itself; whoever drives it
//! passes a [`Clock`]. Tests inject a clock of their own and get
//! reproducible event timestamps; the binary passes [`SystemClock`].

use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    /// Wall time: when something happened, as a person reads it.
    fn now(&self) -> DateTime<Utc>;

    /// Time that stands still while the host is suspended — how long the
    /// machine has been awake. The wall clock outrunning it by more than
    /// the time between two readings is what says the host slept.
    ///
    /// The process's own monotonic clock does not advance while the host
    /// is suspended on the systems the engine runs on, so that is what a
    /// clock reads unless it says otherwise. A clock whose wall time is
    /// frozen and whose awake time is real reads as a host that never
    /// slept.
    fn awake(&self) -> std::time::Instant {
        std::time::Instant::now()
    }
}

/// The real wall clock — the imperative shell's implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
