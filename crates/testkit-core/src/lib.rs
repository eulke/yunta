//! Test scaffolding for the crates that depend on `yunta-core` alone.
//!
//! `yunta-core` and `yunta-adapters` sit below the run harness — there is no
//! run, no storage and no engine at their level — so what their tests need to
//! make time, identity and a session request constant lives here, once, and
//! nothing that needs a run. `yunta-testkit` builds on it with everything a
//! whole run takes — a git repository, a bench, a pty, a binary runner.
//!
//! Its map: `adapter` is what a test of an adapter needs first; `clock` and
//! `ids` are deterministic clocks and ids; `capture` is a writer a test reads
//! back; `golden` compares a surface's output with the output written down for
//! it; `log` writes a log by hand; `persisted` states what every persisted
//! document promises; `repo` asks git directly; `spell` is the spelling a
//! surface writes next steps with; and `stubs` are the scripted CLIs that stand
//! in for a real `codex` or `claude`.
//!
//! It is a dev-dependency only: nothing here ships in a published crate.

pub mod adapter;
mod capture;
mod clock;
pub mod golden;
mod ids;
mod log;
pub mod persisted;
pub mod repo;
mod spell;
pub mod stubs;

pub use capture::Captured;
pub use clock::{fixed_now, AtClock, FixedClock, HostClock, RushingClock, FIXED_NOW};
pub use ids::SeqIdSource;
pub use log::Log;
pub use spell::spell;
pub use yunta_core::events::sample_payloads as all_kinds;
