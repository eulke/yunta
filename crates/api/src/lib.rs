//! What can be asked of Yunta: each operation an interface asks for, done
//! with the engine, the store and the adapters, handed a [`Workspace`] and
//! returning data. It prints nothing, knows no transport, and reads nothing
//! of its own process: an interface reads its process once and hands the
//! workspace in.
//!
//! Every interface plugs into it and gets the same behavior: the CLI, the
//! MCP server an agent asks Yunta through, and later the terminal
//! application and the desktop. A decision two of them would make is made
//! here, once.
//!
//! Its map:
//!
//! - What an interface hands in: `workspace`, the [`Workspace`] it works in
//!   with the boundaries every decision is injected through, and `stop`,
//!   the [`Stop`] it stops an operation with.
//! - What a workspace resolves: `project`, the [`Project`] rooted where it
//!   works, its config layers merged and the roots its runs live under;
//!   and `catalog`, the workflows its repository and installed packs offer.
//! - What a project's runs are: `field`, the [`FieldReading`] of the runs of
//!   its repository or of this machine, as [`FieldReach`] asks; `run`, one
//!   of them found by the [`RunRef`] a person typed, opened as an
//!   [`OpenedRun`], read as a [`RunReading`] or a [`StatsReading`],
//!   receipted, verified, and followed through its [`RunRegistry`] and the
//!   [`Words`] its sessions say; `event_log`, the [`EventLog`] a follower
//!   reads a moving run's log through; and `history`, a
//!   workflow's past runs as a [`WorkflowHistory`].
//! - What this machine answers for: `machine`, the adapters this binary
//!   builds.
//! - What it hands back when it does not do what it was asked: `error`, an
//!   [`Error`] that is a refusal render says, a [`Failure`] below it, or the
//!   caller's stop.

mod catalog;
mod error;
mod event_log;
mod field;
mod history;
pub mod machine;
mod project;
mod run;
mod stop;
mod workspace;

pub use error::{Error, Failure};
pub use event_log::EventLog;
pub use field::{offers_a_menu, FieldReach, FieldReading};
pub use history::WorkflowHistory;
pub use project::{config_layers, Project};
pub use run::{OpenedRun, RunReading, RunRef, RunRegistry, StatsReading, Words};
pub use stop::Stop;
pub use workspace::Workspace;
