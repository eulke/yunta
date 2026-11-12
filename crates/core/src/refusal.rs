//! What Yunta refuses, as data: to a run, a decision it will not record, a
//! reply to its questions it will not take, a close it will not make; to a
//! project, a pack it will not install, update, remove or audit, and a file
//! setting it up will not write. The engine and the API return it; render
//! says it, with the commands that go on from it.
//!
//! Not a failure: a refusal is what is true of the run or of the request, so
//! a person reads it and does something else. A failure is something below
//! that broke (the store, a document), said in its own words with its causes.

use std::path::PathBuf;

use crate::diagnostic::Report;
use crate::events::{Asks, Withheld};
use crate::{AdapterId, NodeId, OptionId, PackRef, Publisher, RunnerName, SchemaRange};

/// Why a decision put to a run is not recorded. Each variant's display is
/// its code; the words a person reads are render's.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GateRefusal {
    /// The run is not stopped at a pause: a live process may drive it,
    /// or it already finished.
    #[error("not_paused")]
    NotPaused,
    /// The run is paused for a reason with no menu of options to answer.
    #[error("nothing_to_resolve")]
    NothingToResolve,
    /// The chosen option is not on the menu; `declared` lists the options it
    /// holds, in the order it offers them.
    #[error("unknown_option")]
    UnknownOption {
        chosen: OptionId,
        declared: Vec<OptionId>,
    },
    /// The option sends the run back to a session, and the words it asks for
    /// are the one thing that session would get.
    #[error("unsaid")]
    Unsaid { chosen: OptionId, asks: Asks },
    /// The gate does not offer the option, and `withheld` says why.
    #[error("withheld")]
    Withheld {
        chosen: OptionId,
        withheld: Withheld,
    },
}

/// Why a run is not closed. Each variant's display is its code; the words a
/// person reads are render's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CloseRefusal {
    /// Its log already says it closed.
    #[error("already_closed")]
    AlreadyClosed,
    /// An engine that lives drives it.
    #[error("driven")]
    Driven,
    /// Its log says it is moving and nothing says the engine that drove
    /// it is gone: it may be between two processes.
    #[error("moving")]
    Moving,
}

/// Why a reply to a node's questions is not recorded. Each variant's display
/// is its code; the words a person reads are render's.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AnswersRefusal {
    /// The node has no open round. Either it never asked, or its questions
    /// are already answered, and a second reply to a closed round is not one.
    #[error("nothing_asked")]
    NothingAsked { node: NodeId },
    /// The reply does not answer the questions it claims to, every way it
    /// fails named. Nothing was written: a surface corrects and replies again.
    #[error("rejected")]
    Rejected(Report),
}

/// What this binary cannot serve for a run, said before the run exists or a
/// resume takes anything: each names the node, runner or adapter a reader
/// changes. Each variant's display is its code; the words a person reads are
/// render's.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unservable {
    /// The run is pinned to an adapter that a runner the node uses does not
    /// reach: none of that runner's candidates is on it.
    #[error("no_candidate_on_pin")]
    NoCandidateOnPin {
        node: NodeId,
        runner: RunnerName,
        pin: AdapterId,
        candidates: Vec<AdapterId>,
    },
    /// The pinned adapter is not one `runners:` names that this binary
    /// builds; `runnable` lists those it does, sorted.
    #[error("unbuilt_pin")]
    UnbuiltPin {
        pin: AdapterId,
        runnable: Vec<AdapterId>,
    },
    /// A mock run written before runs froze their adapter: its log shows it
    /// was scripted, and nothing kept the fixture it was scripted from.
    #[error("mock_before_frozen_adapter")]
    MockBeforeFrozenAdapter,
    /// Nodes that open sessions, and no adapter `runners:` names that this
    /// binary builds; `built` names the ones it builds.
    #[error("no_adapter")]
    NoAdapter { built: Vec<AdapterId> },
    /// Adapters the run reaches that answered their health check unhealthy,
    /// each by its name and what it said.
    #[error("unhealthy")]
    Unhealthy { adapters: Vec<(String, String)> },
}

/// Why a pack command refuses: what the pack is, where it stands, or the
/// policy the project's config sets on packs. Each variant's display is its
/// code; the words a person reads are render's.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackRefusal {
    /// An `add` of a pack this project already has vendored at `at`.
    #[error("already_installed")]
    AlreadyInstalled { pack: PackRef, at: PathBuf },
    /// An `update` of a pack this project's lock does not name.
    #[error("not_installed")]
    NotInstalled { pack: PackRef },
    /// A `remove` of a pack this project's lock does not name.
    #[error("nothing_to_remove")]
    NothingToRemove { pack: PackRef },
    /// An `audit` of a pack not vendored under `root`.
    #[error("not_vendored")]
    NotVendored { pack: PackRef, root: PathBuf },
    /// A pack whose `yunta_schema` range leaves out the schema this binary
    /// speaks.
    #[error("schema_outside")]
    SchemaOutside {
        pack: PackRef,
        range: SchemaRange,
        speaks: u32,
    },
    /// A pack from a publisher outside the allow-list the config layers
    /// `declared_by` set.
    #[error("publisher_not_allowed")]
    PublisherNotAllowed {
        publisher: Publisher,
        allowed: Vec<Publisher>,
        declared_by: Vec<&'static str>,
    },
    /// A pack declaring `executors` under an executor policy of `deny`, which
    /// the config layers `declared_by` set: no confirmation lifts it.
    #[error("executors_denied")]
    ExecutorsDenied {
        executors: usize,
        declared_by: Vec<&'static str>,
        verb: PackVerb,
    },
    /// A pack declaring `executors` under an executor policy that asks a
    /// person, who has not confirmed.
    #[error("executors_unconfirmed")]
    ExecutorsUnconfirmed { executors: usize, verb: PackVerb },
    /// An `update` of `pack` to `reference`, cloned from where its lock
    /// records it, whose manifest names another pack, `is`.
    #[error("another_pack")]
    AnotherPack {
        pack: PackRef,
        reference: String,
        cloned_from: String,
        is: PackRef,
    },
    /// A `new` pack where a directory already stands.
    #[error("directory_exists")]
    DirectoryExists { dir: PathBuf },
}

/// Why setting a repository up refuses: a file it would write that is there
/// already, or a workflow it is asked to start that cannot be named so. Each
/// variant's display is its code; the words a person reads are render's.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetupRefusal {
    /// A file `init` or `new` would write that is at `path` already, and
    /// nothing said to write it again.
    #[error("exists")]
    Exists { path: PathBuf },
    /// A new workflow's name that is empty.
    #[error("empty_name")]
    EmptyName,
    /// A new workflow's name with a character its file's stem may not hold,
    /// so `<name>.yaml` could leave `.yunta/workflows/`.
    #[error("unsafe_name")]
    UnsafeName { name: String },
    /// A shape no skeleton has, beside the ones `shapes` names, the plainest
    /// first.
    #[error("unknown_shape")]
    UnknownShape { name: String, shapes: Vec<String> },
}

/// Why a project's cases do not run: none where they were looked for. Each
/// variant's display is its code; the words a person reads are render's.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CasesRefusal {
    /// The directory the cases are looked for in, `dir`, does not read.
    #[error("unreadable")]
    Unreadable { dir: PathBuf },
    /// The directory the cases are looked for in, `dir`, holds none.
    #[error("none_found")]
    NoneFound { dir: PathBuf },
}

/// What a pack command was asked to do to a pack, as a refusal names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackVerb {
    Install,
    Update,
}
