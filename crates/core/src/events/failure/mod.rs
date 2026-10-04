//! Why a node failed, as data.
//!
//! The one payload field that is not a plain value: a node fails either
//! with a sentence the engine states, or with declared artifacts that
//! did not close, each saying what went wrong with it. Keeping both in
//! one type is what lets the log record the facts and every surface
//! produce its own prose from them, instead of the engine writing prose
//! once and three surfaces taking it apart again.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::diagnostic::{ArtifactFailure, Report};
use crate::glob::{listed_globs, InvalidScopeGlob, ScopeGlob};
use crate::hash::ContentHash;
use crate::ids::AdapterId;

mod exit;

pub use exit::{CommandExit, CommandOrigin, SessionDeath, SessionEnd, SessionExit, TAIL_LINES};

/// Why a node failed.
///
/// Untagged, with `Message` last: a payload carrying `artifacts:` reads
/// as [`Failure::Artifacts`], one carrying `died:` as
/// [`Failure::SessionDied`], one carrying `exited:` as
/// [`Failure::Exited`], one carrying `outside_scope:` as
/// [`Failure::ScopeViolated`], one carrying `requested_scope:` as
/// [`Failure::ScopeRequested`], one carrying `unset:` as
/// [`Failure::Unset`], one carrying `unchanged:` as
/// [`Failure::Unchanged`], one carrying `denied_paths:` as
/// [`Failure::PathsDenied`], and a log written before failures were
/// data carries `outcome:` alone and reads back as
/// [`Failure::Message`]. That tolerance is the rule for what is
/// persisted and versioned, and it is why no reader needs to know which
/// version wrote the line it is looking at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Failure {
    /// Declared artifacts that did not close, each saying what went
    /// wrong with it: the file, the content of the document, or the run
    /// that owes the artifact and holds none of it.
    Artifacts { artifacts: Vec<ArtifactFailure> },
    /// The session the node was working in ended without a terminal
    /// event, and how its process went.
    SessionDied { died: SessionDeath },
    /// The node's diff reached paths no glob of its scope allows. Named
    /// one by one, because what a person does next — widen the scope,
    /// or change those files in the run's tree — is about exactly them.
    ScopeViolated { outside_scope: Vec<PathBuf> },
    /// The node's session asked for more scope than it has: its work is
    /// not done until a person answers, and the answer is theirs.
    ScopeRequested { requested_scope: RequestedScope },
    /// Tasks of a loop asked for scope beyond their own, and nobody was
    /// there to answer: each request is owed a person's decision, and
    /// nothing else of the loop can run until it gets one.
    ScopeOwed { owed: Vec<OwedScope> },
    /// A config key the node cannot run without, left unset in the
    /// config the run froze when it was created — so no attempt of this
    /// run can go differently.
    Unset { unset: crate::config::ConfigKey },
    /// A check that judges the run's tree was asked to run again on the
    /// very tree an earlier attempt failed on: it is refused before
    /// running, since the same command on the same tree answers the same.
    Unchanged { unchanged: Unchanged },
    /// The work reached paths no session of the run may write: what the
    /// project denies to every run (`permissions.paths.deny`), or a test a
    /// person approved. No grant widens a deny, so a person is never
    /// offered one: the work goes, or the project changes its
    /// config.
    PathsDenied { denied_paths: Vec<PathBuf> },
    /// A command the node ran exited non-zero, and what it printed last
    /// is the reason a person reads.
    Exited { exited: CommandExit },
    /// A failure the engine states in one sentence.
    Message { outcome: String },
}

/// The attempt that last actually ran on the tree a refused attempt
/// would have run on, and what it failed with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Unchanged {
    pub since: u32,
    pub failure: Box<Failure>,
}

impl Unchanged {
    /// Why the attempt was not run, ending in what the attempt that last
    /// ran failed with, as `failed` says it.
    fn sentence(&self, failed: &str) -> String {
        format!(
            "not run again: nothing in the run's tree changed since attempt {} failed on it — \
             change what it failed on there first. Attempt {} failed: {failed}",
            self.since, self.since
        )
    }
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

impl Failure {
    pub fn message(text: impl Into<String>) -> Self {
        Failure::Message {
            outcome: text.into(),
        }
    }

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
    pub fn unset(key: crate::config::ConfigKey) -> Self {
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

    /// Whether another attempt of the node, in this same run, can end
    /// differently. Not when the cause is the config the run froze at
    /// birth: every attempt reads the same one, and the way out is a new
    /// run under a config that declares what is missing.
    pub fn retry_can_change(&self) -> bool {
        !matches!(self, Failure::Unset { .. })
    }

    /// A command that exited non-zero, and what it left behind.
    pub fn exited(exited: CommandExit) -> Self {
        Failure::Exited { exited }
    }

    /// The failure as one claim, without the lines a failing command
    /// printed: what a surface states before it lists them.
    pub fn headline(&self) -> String {
        match self {
            Failure::Exited { exited } => exited.headline(),
            Failure::Unchanged { unchanged } => unchanged.sentence(&unchanged.failure.headline()),
            // Each document that did not close, by the heading its own
            // block opens with: the path and how many problems it has.
            Failure::Artifacts { artifacts } => artifacts
                .iter()
                .filter_map(|artifact| artifact.to_string().lines().next().map(str::to_string))
                .collect::<Vec<_>>()
                .join("; "),
            // How many files, when there are several to list.
            Failure::ScopeViolated { outside_scope } if outside_scope.len() > 1 => format!(
                "scope violated: {} outside the declared globs",
                crate::text::counted(outside_scope.len(), "file")
            ),
            Failure::SessionDied { .. }
            | Failure::ScopeViolated { .. }
            | Failure::ScopeRequested { .. }
            | Failure::ScopeOwed { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::Message { .. } => self.to_string(),
        }
    }

    /// What the failure says beyond its [`Failure::headline`], a line
    /// each: the last lines a command printed, the problems of a document
    /// that did not close — each document's own block when several did —
    /// and every path that fell outside a scope when there are several.
    pub fn detail(&self) -> Vec<String> {
        match self {
            Failure::Exited { exited } => exited.tail.clone(),
            Failure::Unchanged { unchanged } => unchanged.failure.detail(),
            Failure::Artifacts { artifacts } => {
                let blocks = artifacts.iter().map(ToString::to_string);
                let lines: Vec<String> = match artifacts.len() {
                    1 => blocks.flat_map(|block| own_lines(&block, 1)).collect(),
                    _ => blocks.flat_map(|block| own_lines(&block, 0)).collect(),
                };
                lines
            }
            Failure::ScopeViolated { outside_scope } if outside_scope.len() > 1 => outside_scope
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            Failure::ScopeOwed { owed } => owed
                .iter()
                .map(|owed| {
                    format!(
                        "`{}` — {}: {}",
                        owed.task_id,
                        listed_globs(&owed.paths),
                        owed.reason
                    )
                })
                .collect(),
            Failure::SessionDied { .. }
            | Failure::ScopeViolated { .. }
            | Failure::ScopeRequested { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::Message { .. } => Vec::new(),
        }
    }

    /// The last lines the failing command printed. Empty for a failure
    /// that is not about a command, or one that printed nothing.
    pub fn tail(&self) -> &[String] {
        match self {
            Failure::Exited { exited } => &exited.tail,
            Failure::Unchanged { unchanged } => unchanged.failure.tail(),
            Failure::Artifacts { .. }
            | Failure::SessionDied { .. }
            | Failure::ScopeViolated { .. }
            | Failure::ScopeRequested { .. }
            | Failure::ScopeOwed { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::Message { .. } => &[],
        }
    }

    /// The run object holding everything the failing command printed.
    pub fn output(&self) -> Option<&ContentHash> {
        match self {
            Failure::Exited { exited } => exited.output.as_ref(),
            Failure::Unchanged { unchanged } => unchanged.failure.output(),
            Failure::Artifacts { .. }
            | Failure::SessionDied { .. }
            | Failure::ScopeViolated { .. }
            | Failure::ScopeRequested { .. }
            | Failure::ScopeOwed { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::Message { .. } => None,
        }
    }

    /// Work that reached `paths`, which no session of the run may write.
    pub fn paths_denied(paths: Vec<PathBuf>) -> Self {
        Failure::PathsDenied {
            denied_paths: paths,
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

    /// The paths the node wrote outside its scope. Empty for a failure
    /// that is not a violation of it.
    pub fn outside_scope(&self) -> &[PathBuf] {
        match self {
            Failure::ScopeViolated { outside_scope } => outside_scope,
            Failure::Unchanged { unchanged } => unchanged.failure.outside_scope(),
            Failure::Artifacts { .. }
            | Failure::SessionDied { .. }
            | Failure::ScopeRequested { .. }
            | Failure::ScopeOwed { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::Exited { .. }
            | Failure::Message { .. } => &[],
        }
    }

    /// Whether a wider scope is what this failure needs — what makes a
    /// grant a way forward rather than a guess.
    pub fn wants_scope(&self) -> bool {
        match self {
            Failure::ScopeViolated { outside_scope } => !outside_scope.is_empty(),
            Failure::ScopeRequested { requested_scope } => !requested_scope.paths.is_empty(),
            Failure::ScopeOwed { owed } => owed.iter().any(|owed| !owed.paths.is_empty()),
            Failure::Unchanged { unchanged } => unchanged.failure.wants_scope(),
            Failure::Artifacts { .. }
            | Failure::SessionDied { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::Exited { .. }
            | Failure::Message { .. } => false,
        }
    }

    /// What a grant would add to the node's scope for the work behind
    /// this failure to stand: each path it wrote outside, exactly, or
    /// what its session asked for. Empty for a failure a wider scope
    /// would not change.
    pub fn scope_wanted(&self) -> Result<Vec<ScopeGlob>, InvalidScopeGlob> {
        match self {
            Failure::ScopeRequested { requested_scope } => Ok(requested_scope.paths.clone()),
            // Each task's request is its own to grant: none of it widens
            // the loop's scope.
            Failure::ScopeOwed { .. } => Ok(Vec::new()),
            _ => self
                .outside_scope()
                .iter()
                .map(|path| ScopeGlob::exact(path))
                .collect(),
        }
    }

    /// [`scope_wanted`](Self::scope_wanted) as a sentence lists it: the
    /// paths as a person reads them, comma-separated.
    pub fn scope_wanted_listed(&self) -> String {
        match self {
            Failure::ScopeRequested { requested_scope } => listed_globs(&requested_scope.paths),
            Failure::ScopeOwed { owed } => listed_globs(&owed_paths(owed)),
            _ => self
                .outside_scope()
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    /// Every report behind this failure, each carrying the document it
    /// is about. What a diagnostic is rendered from; a failure whose
    /// artifacts name no document yields none.
    pub fn reports(&self) -> impl Iterator<Item = &Report> {
        self.failures().filter_map(ArtifactFailure::report)
    }

    /// Every declared artifact that did not close. Empty for a failure
    /// that has nothing to do with artifacts.
    pub fn failures(&self) -> impl Iterator<Item = &ArtifactFailure> {
        match self {
            Failure::Artifacts { artifacts } => artifacts.iter(),
            Failure::Unchanged { unchanged } => unchanged.failure.failures(),
            // A dead session names no artifact, and neither does a
            // scope, a command or a sentence: the count of documents
            // that did not close is about documents this node declared.
            Failure::SessionDied { .. }
            | Failure::ScopeViolated { .. }
            | Failure::ScopeRequested { .. }
            | Failure::ScopeOwed { .. }
            | Failure::Unset { .. }
            | Failure::PathsDenied { .. }
            | Failure::Exited { .. }
            | Failure::Message { .. } => [].iter(),
        }
    }
}

impl fmt::Display for Failure {
    /// The prose a reader sees, produced here rather than stored: one
    /// sentence for a plain failure, and one block per failing document
    /// in declaration order for an artifact failure.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Message { outcome } => f.write_str(outcome),
            Failure::Exited { exited } => write!(f, "{exited}"),
            Failure::Unset { unset } => write!(f, "{unset}"),
            Failure::Unchanged { unchanged } => {
                f.write_str(&unchanged.sentence(&unchanged.failure.to_string()))
            }
            Failure::SessionDied { died } => write!(f, "{died}"),
            Failure::ScopeViolated { outside_scope } => {
                write!(
                    f,
                    "scope violated: {} outside the declared globs — ",
                    crate::text::counted(outside_scope.len(), "file")
                )?;
                for (position, path) in outside_scope.iter().enumerate() {
                    if position > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", path.display())?;
                }
                Ok(())
            }
            Failure::PathsDenied { denied_paths } => write!(
                f,
                "wrote what no session of the run may write (`permissions.paths.deny`, or a \
                 test a person approved) — {}; no grant widens it: undo those changes",
                denied_paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Failure::ScopeRequested { requested_scope } => write!(
                f,
                "asked for scope beyond its own — {}: {}",
                listed_globs(&requested_scope.paths),
                requested_scope.reason
            ),
            Failure::ScopeOwed { owed } => f.write_str(&owed_sentence(owed)),
            Failure::Artifacts { artifacts } => {
                for (position, artifact) in artifacts.iter().enumerate() {
                    if position > 0 {
                        f.write_str("\n")?;
                    }
                    write!(f, "{artifact}")?;
                }
                Ok(())
            }
        }
    }
}

/// The requests a loop owes a person, as one sentence.
fn owed_sentence(owed: &[OwedScope]) -> String {
    let each: Vec<String> = owed
        .iter()
        .map(|owed| {
            format!(
                "task `{}` asked for {}",
                owed.task_id,
                listed_globs(&owed.paths)
            )
        })
        .collect();
    format!(
        "{} owe a person's decision about scope: {}",
        crate::text::counted(owed.len(), "request"),
        each.join("; ")
    )
}

/// Every path the owed requests ask for, each once.
fn owed_paths(owed: &[OwedScope]) -> Vec<ScopeGlob> {
    let mut paths: Vec<ScopeGlob> = Vec::new();
    for path in owed.iter().flat_map(|owed| owed.paths.iter()) {
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
    paths
}

/// The lines of `block` after its first `skip`, without their indent.
fn own_lines(block: &str, skip: usize) -> Vec<String> {
    block
        .lines()
        .skip(skip)
        .map(|line| line.trim().to_string())
        .collect()
}
