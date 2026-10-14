//! The records a failure carries and the constructors that take them up: the
//! attempt an unchanged one repeats, the scope and the criterion a node or a
//! task asks a person about, a red check's figures, what a refused child says
//! of itself, the typed causes behind the engine's own failures, and the
//! parts a join, a runner, a capability and a wall-clock bound name. A surface
//! words each of them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::diagnostic::{ArtifactFailure, Report};
use crate::glob::ScopeGlob;
use crate::hash::ContentHash;
use crate::ids::{AdapterId, ExecutorName, NodeId, RunId};
use crate::inputs::InputsProblem;
use crate::protected::Protection;
use crate::workflow::{ArtifactKind, ArtifactRefId, ReservedIdentity, OBJECTS_DIR};

use super::{CommandExit, Failure, SessionDeath, SessionExit};

/// The attempt that last actually ran on the tree a refused attempt
/// would have run on, and what it failed with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Unchanged {
    pub since: u32,
    pub failure: Box<Failure>,
}

/// One task's request for scope, owed a person's decision: what it asked
/// to be allowed to write, and why, in its session's own words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OwedScope {
    pub task_id: crate::TaskId,
    pub paths: Vec<ScopeGlob>,
    pub reason: String,
}

/// What a node's session asked to be allowed to write, and why, in its
/// own words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RequestedScope {
    pub paths: Vec<ScopeGlob>,
    pub reason: String,
}

/// A task whose declared criterion no work of the plan can pass: the
/// criterion's command, and the reason its session declared it with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UnpassableTask {
    pub task_id: crate::TaskId,
    pub cmd: String,
    pub why: String,
}

/// Why a judging check came out red: a coverage figure under its
/// threshold or none at all, a command that exited non-zero, or a baseline
/// suite that passed when it was measured and fails in this check.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RedCheck {
    /// The coverage the run measured, when it measured one, and the
    /// threshold it fell short of. `cmd` is the command that measured it.
    Coverage {
        cmd: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        measured: Option<f64>,
        threshold: f64,
    },
    /// The command that exited non-zero, with its exit code, the line it
    /// concluded with and the run object holding everything it printed.
    Command {
        cmd: String,
        exit_code: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        conclusion: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<ContentHash>,
    },
    /// The baseline suite passed when the lineage measured it and fails in this check;
    /// `tail` is what it printed last, which says why.
    Regression {
        cmd: String,
        exit_code: i32,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tail: Vec<String>,
    },
}

/// Why a composed workflow the run could not start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "refusal", rename_all = "snake_case")]
pub enum ChildRefusal {
    /// No workflow answers to the name; `problem` is the catalog's own account.
    Unresolved { problem: String },
    /// Its file does not read as a workflow.
    Invalid { report: Report },
    /// Its own check refuses it; `rules` are the codes of what it refused.
    FailsCheck { rules: Vec<String> },
    /// The inputs the node hands it do not fit what it declares; `problem`
    /// says what does not fit.
    Inputs { problem: InputsProblem },
}

/// Why the bytes an artifact names cannot be handed over: the run's store
/// holds no object under the hash, or the object holds other bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectProblem {
    /// The run holds no object under `hash`.
    Missing { hash: ContentHash },
    /// The object under `hash` holds content that hashes to `found`: these
    /// are not the bytes the run accepted.
    Corrupt {
        hash: ContentHash,
        found: ContentHash,
    },
}

impl ObjectProblem {
    /// What the problem says of the object, in the words every surface
    /// shows: the run holds no object under the hash, or the object under
    /// it holds other bytes than the run accepted.
    pub fn sentence(&self) -> String {
        match self {
            ObjectProblem::Missing { hash } => format!(
                "the run holds no object `{hash}` — an artifact names bytes that are not under \
                 `{OBJECTS_DIR}/`"
            ),
            // The one thing a content-addressed store can state and a path
            // never could: these are not the bytes that were accepted.
            ObjectProblem::Corrupt { hash, found } => format!(
                "object `{hash}` holds content that hashes to `{found}` — the bytes under \
                 `{OBJECTS_DIR}/` are not the bytes the run accepted"
            ),
        }
    }
}

/// Why a mount cannot bring the artifact a node names into the node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MountError {
    /// No linked child run of `node` reached a terminal state in this run.
    NoTerminalChild {
        artifact: ArtifactRefId,
        node: NodeId,
    },
    /// The child run of `node` cannot hand over the bytes the mount names.
    Unreadable {
        artifact: ArtifactRefId,
        node: NodeId,
        run: RunId,
        source: ObjectProblem,
    },
    /// The frozen truth of the child run cannot be read; `detail` says why.
    Unfrozen {
        artifact: ArtifactRefId,
        node: NodeId,
        run: RunId,
        detail: String,
    },
    /// The child run's log cannot be read for the artifact; `source` is the
    /// storage's own account of why, when it gave one.
    Unlogged {
        artifact: ArtifactRefId,
        node: NodeId,
        run: RunId,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        source: String,
    },
}

/// Why a name cannot name an artifact: the three rules an artifact name keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NameProblem {
    /// The name is empty: an artifact is named by a file name.
    Empty,
    /// The name is absolute or climbs with `..`, so it leaves the run directory.
    OutsideRun,
    /// The name is the one the run's view writes for `identity`.
    Reserved { identity: ReservedIdentity },
}

/// Why a frozen read cannot give a node what it asked for: a name that
/// resolves to nothing, a pack path that leaves its pack, or a prompt file
/// that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FrozenRead {
    /// A name that resolves to nothing, or to a definition that cannot be read;
    /// `problem` is the resolver's own account.
    Unresolved { problem: String },
    /// A path a pack declares that leaves the pack's own directory.
    Escapes { pack: String, path: String },
    /// A prompt file a node reads that cannot be read.
    Prompt { node: NodeId, path: PathBuf },
}

/// Why a `join: any` race's loser stopped: a sibling in its group finished first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Interruption {
    /// A sibling in the same group succeeded first, and this child lost the race.
    SiblingWon,
}

/// Why a node's runner did not resolve to an adapter a run can use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerRefusal {
    /// No `runners:` entry defines the runner.
    Unknown,
    /// None of the runner's candidates is available; `tried` names the adapters it looked at.
    NoCandidate { tried: Vec<AdapterId> },
}

/// What a node needs a capability for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CapabilityUse {
    /// The node is in a blackboard group, which the run tools carry.
    Blackboard,
    /// The node declares a typed artifact of `kind`, which the run tools carry.
    TypedArtifact { kind: ArtifactKind },
    /// The node is a loop, whose task sessions read their task through the run tools.
    LoopTasks,
}

/// The wall-clock bound a run's work went past: a session's own, or an executor's.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Timed {
    /// A session's wall-clock bound.
    #[default]
    Session,
    /// An executor's wall-clock bound.
    Executor { executor: ExecutorName },
}

impl Failure {
    pub fn artifacts(failures: Vec<ArtifactFailure>) -> Self {
        Failure::Artifacts {
            artifacts: failures,
        }
    }

    /// A session of `adapter` that ended without a terminal event.
    /// `exit` is what its process left behind, absent for a session
    /// with no process of its own.
    pub fn session_died(adapter: AdapterId, exit: Option<SessionExit>) -> Self {
        Failure::SessionDied {
            died: SessionDeath { adapter, exit },
        }
    }

    /// A diff that reached `outside_scope`, which the node's scope does
    /// not allow.
    pub fn scope_violated(outside_scope: Vec<PathBuf>) -> Self {
        Failure::ScopeViolated { outside_scope }
    }

    /// A node whose kind cannot run without `key`, which the run's config
    /// leaves unset.
    pub fn unset(key: crate::ConfigKey) -> Self {
        Failure::Unset { unset: key }
    }

    /// An attempt refused because nothing changed in the run's tree
    /// since attempt `since` failed on it with `failure`. A refusal of a
    /// refusal names the attempt that last ran, and what it failed with.
    pub fn unchanged(since: u32, failure: Failure) -> Self {
        match failure {
            Failure::Unchanged { unchanged } => Failure::Unchanged { unchanged },
            failure => Failure::Unchanged {
                unchanged: Unchanged {
                    since,
                    failure: Box::new(failure),
                },
            },
        }
    }

    /// A command that exited non-zero, and what it left behind.
    pub fn exited(exited: CommandExit) -> Self {
        Failure::Exited { exited }
    }

    /// Work that reached `paths`, which no session of the run may write,
    /// each path with the reason at the same index in `reasons`.
    pub fn paths_denied(paths: Vec<PathBuf>, reasons: Vec<Protection>) -> Self {
        Failure::PathsDenied {
            denied_paths: paths,
            reasons,
        }
    }

    /// A session that asked to be allowed `paths`, for `reason`.
    pub fn scope_requested(paths: Vec<ScopeGlob>, reason: impl Into<String>) -> Self {
        Failure::ScopeRequested {
            requested_scope: RequestedScope {
                paths,
                reason: reason.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_object_problem_says_what_the_store_holds_and_what_it_found() {
        let hash = ContentHash::sha256(b"accepted");
        let found = ContentHash::sha256(b"changed");
        assert_eq!(
            ObjectProblem::Missing { hash: hash.clone() }.sentence(),
            format!(
                "the run holds no object `{hash}` — an artifact names bytes that are not under \
                 `objects/`"
            )
        );
        assert_eq!(
            ObjectProblem::Corrupt {
                hash: hash.clone(),
                found: found.clone(),
            }
            .sentence(),
            format!(
                "object `{hash}` holds content that hashes to `{found}` — the bytes under \
                 `objects/` are not the bytes the run accepted"
            )
        );
    }
}
