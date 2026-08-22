//! Injected time (CLAUDE.md: "Determinismo inyectado") — the engine's
//! decision logic never calls `Utc::now()` itself; whoever drives it
//! passes a [`Clock`]. Tests inject a fixed or stepping clock and get
//! reproducible event timestamps; the binary passes [`SystemClock`].

use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// The real wall clock — the imperative shell's implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
