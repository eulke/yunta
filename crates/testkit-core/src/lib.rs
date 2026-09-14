//! Test scaffolding for the crates that depend on `yunta-core` alone.
//!
//! `yunta-core` and `yunta-adapters` sit below the run harness — there is
//! no run, no storage and no engine at their level — and used to hand-roll
//! their own clocks, id sources and session requests, one copy per test
//! file. This crate is the one copy: what a test needs to make time,
//! identity and a session request constant, and nothing that needs a run.
//!
//! `yunta-testkit` builds on it with everything a whole run takes — a git
//! repository, a bench, a pty, a binary runner.
//!
//! It is a dev-dependency only: nothing here ships in a published crate.

pub mod adapter;
mod capture;
mod clock;
mod ids;
mod kinds;
pub mod persisted;

pub use capture::Captured;
pub use clock::{AtClock, FixedClock, FIXED_NOW};
pub use ids::SeqIdSource;
pub use kinds::all_kinds;
