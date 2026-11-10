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
//!   `catalog`, the workflows its repository and installed packs offer;
//!   `packs`, the repository's [`Packs`], fetched, installed, updated,
//!   removed, listed, audited and scaffolded; and `setup`, the repository
//!   set up for Yunta, a [`DetectedProject`] written by `init` and a new
//!   workflow written from its [`WorkflowShape`].
//! - What a repository's cases do: `cases`, the [`CaseSuite`] under a
//!   directory's `.yunta/tests/`, each run in a sandbox of its own with its
//!   sessions scripted, and held to what it expects.
//! - What a project's runs are: `field`, the [`FieldReading`] of the runs of
//!   its repository or of this machine, as [`FieldReach`] asks; `run`, one
//!   of them found by the [`RunRef`] a person typed, opened as an
//!   [`OpenedRun`], read as a [`RunReading`] or a [`StatsReading`],
//!   receipted, verified, and followed through its [`RunRegistry`] and the
//!   [`Words`] its sessions say; `event_log`, the [`EventLog`] a follower
//!   reads a moving run's log through; and `history`, a
//!   workflow's past runs as a [`WorkflowHistory`].
//! - What a workflow gets here: `workflow`, the file a reference names, read
//!   as a [`WorkflowFile`] or audited whole as an [`AuditedWorkflow`];
//!   `verdict`, the [`Verdict`] on whether it can run, as deep as
//!   [`VerdictReach`] asks; and `admission`, a run's
//!   [`Admission`] of a [`RunRequest`], made a [`CreatedRun`] (`creation`),
//!   held as a [`HeldRun`] (`hold`, a parked run taken back too), driven
//!   here with the [`Ports`] a surface lends to a [`Driven`] run (`drive`),
//!   or handed to a detached Yunta (`detach`); and `answers`, the
//!   [`WaitingDecision`] a run waits on, decided or its questions answered;
//!   and `ending`, a run cancelled, its engine and process groups stopped,
//!   or closed for good; and `collect`, what a project's runs leave
//!   collected as far as [`Collect`] asks; every caution said through the
//!   [`Remarks`] the caller passes (`remark`), git's [`Committer`] among
//!   what it asks (`committer`).
//! - What this machine answers for: `machine`, the adapters this binary
//!   builds, what it refuses to serve a run with, and every check `doctor`
//!   makes of it; `forge`, the forge a run reaches with the workspace's
//!   secrets; and `sandbox`, a checkout of its own a probe run is made in,
//!   apart from the project's.
//! - What it hands back when it does not do what it was asked: `error`, an
//!   [`Error`] that is a refusal render says, a [`Failure`] below it, or the
//!   caller's stop.

mod admission;
mod answers;
mod cases;
mod catalog;
mod collect;
mod committer;
mod creation;
mod detach;
mod drive;
mod ending;
mod error;
mod event_log;
mod field;
mod forge;
mod history;
mod hold;
pub mod machine;
mod packs;
mod project;
mod remark;
mod run;
mod sandbox;
mod setup;
mod stop;
mod verdict;
mod workflow;
mod workspace;

pub use admission::{Admission, RunRequest};
pub use answers::WaitingDecision;
pub use cases::CaseSuite;
pub use collect::Collect;
pub use committer::Committer;
pub use creation::CreatedRun;
pub use drive::{ClosingReading, Driven, Ending, Ports};
pub use ending::ClosedRun;
pub use error::{Error, Failure};
pub use event_log::{EventLog, PromotionChain};
pub use field::{offers_a_menu, FieldReach, FieldReading};
pub use history::WorkflowHistory;
pub use hold::HeldRun;
pub use packs::{AuditedPack, FetchedPack, InstalledPack, Packs, ScaffoldedPack};
pub use project::{config_layers, Project};
pub use remark::{Remark, Remarks};
pub use run::{OpenedRun, RunReading, RunRef, RunRegistry, StatsReading, Words};
pub use setup::{
    DetectedProject, InitializedProject, WorkflowShape, WorkflowStem, WrittenWorkflow,
};
pub use stop::Stop;
pub use verdict::{ContextFiles, Judged, Verdict, VerdictReach};
pub use workflow::{AuditedWorkflow, WorkflowFile};
pub use workspace::Workspace;
