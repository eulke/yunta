//! The one judgement a session's work gets: a task session's when it stops
//! and when its attempt closes, a node's when it stops and when its node
//! closes. Each reads the criteria once, decides the request the session made
//! on the tree it left, audits the diff against the scope in force, and
//! verifies the documents a node owes, so a session is told what its close
//! would say of that same tree.

use std::path::{Path, PathBuf};

use serde::Serialize;
use yunta_core::diagnostic::{ArtifactFailure, FileProblem};
use yunta_core::events::{CriterionUnpassablePayload, Decider, StopRefusal, StoredEvent};
use yunta_core::expansion::{Ask, Bounds, Decision, Denial, Waiting};
use yunta_core::protected::{Closing, Protected};
use yunta_core::{ArtifactSpec, ContentHash, NodeId, NodeKind, ScopeGlob, SessionId, Task, TreeId};

use super::criteria::{post_check, PostCheck};
use super::evidence::{refused_writes, Kept, Located};
use super::memo::Memo;
use super::tree::content_of;
use super::{CriterionRun, SessionObserver, TaskCycleError};
use crate::artifacts::{answerer, Answerer, RunArtifacts};
use crate::process::Supervision;
use crate::scope::{audit_tree, Ceiling, ScopeCheckResult};
use crate::scope_expansion::{GrantWindow, ScopeExpansionOutcome, ScopeExpansionRequest};

mod request;
use request::decide_request;

/// How a judgement settles its request. A dry one decides on a copy of the
/// grant window, so the caller's window never moves; a commit decides on the
/// caller's window, and what it grants counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Settle {
    Dry,
    Commit,
}

/// Where the refused writes a red criterion may locate are read from.
pub(crate) enum Refusals<'a> {
    /// Nothing is read: a node's audit, and a judgement that locates no write.
    None,
    /// The observer holds the refusals, and keeps what the criteria printed.
    Observer {
        observer: &'a dyn SessionObserver,
        node: &'a NodeId,
        sessions: &'a [SessionId],
    },
    /// The run's log holds the refusals, and its directory the output a
    /// criterion kept.
    Log {
        events: &'a [StoredEvent],
        sessions: &'a [SessionId],
        run_dir: &'a Path,
    },
}

/// A document a node's session owes its close, that the close would refuse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DocumentFault {
    pub spec: String,
    pub failure: ArtifactFailure,
    /// The refusal a stop states for this document. It is classified once,
    /// where the fault is found. The state digest reads the failure alone, so
    /// the classification is left out of it.
    #[serde(skip)]
    pub refusal: StopRefusal,
}

impl DocumentFault {
    /// A fault as the judgement states it: a document the session never handed
    /// over is missing, and one it handed over that the close refuses is refused.
    fn of(spec: &ArtifactSpec, failure: ArtifactFailure) -> Self {
        let refusal = match &failure {
            ArtifactFailure::Undelivered { .. }
            | ArtifactFailure::File {
                problem: FileProblem::Missing { .. },
                ..
            } => StopRefusal::ArtifactMissing,
            _ => StopRefusal::ArtifactRefused,
        };
        DocumentFault {
            spec: spec.to_string(),
            failure,
            refusal,
        }
    }
}

/// The documents a node's session hands over, which its close verifies.
pub(crate) struct Documents<'a> {
    pub declared: &'a [ArtifactSpec],
    pub node: &'a NodeId,
    pub node_kind: &'a NodeKind,
    pub run_dir: &'a Path,
    pub max_artifact_bytes: Option<u64>,
    pub events: &'a [StoredEvent],
}

impl Documents<'_> {
    /// Whether a node of `node_kind` hands any of `declared` to its session,
    /// so that its close verifies one: the log is read for them only then.
    pub(crate) fn any_handed(declared: &[ArtifactSpec], node_kind: &NodeKind) -> bool {
        declared.iter().any(|spec| handed_over(node_kind, spec))
    }

    /// Each document the session hands over that its close would refuse. The
    /// close takes the rest from the log, so those are not checked here.
    pub(crate) async fn faults(&self) -> Vec<DocumentFault> {
        let handed: Vec<&ArtifactSpec> = self
            .declared
            .iter()
            .filter(|spec| handed_over(self.node_kind, spec))
            .collect();
        if handed.is_empty() {
            return Vec::new();
        }
        let held = RunArtifacts::of(self.run_dir, self.events);
        let mut faults = Vec::new();
        for spec in handed {
            let verified = crate::run_tools::verify_declared(
                self.node,
                self.node_kind,
                spec,
                &held,
                self.run_dir,
                self.max_artifact_bytes,
            )
            .await;
            if let Err(failure) = verified {
                faults.push(DocumentFault::of(spec, failure));
            }
        }
        faults
    }
}

/// Whether a node of `node_kind` hands `spec` to its session: a file it writes
/// in its staging, or a typed document it submits through a tool.
fn handed_over(node_kind: &NodeKind, spec: &ArtifactSpec) -> bool {
    match answerer(node_kind, spec.kind()) {
        Answerer::Staging => true,
        Answerer::Log => spec.kind().is_some_and(|kind| kind.submit_tool().is_some()),
    }
}

/// What every judgement runs under: the criteria's memo, what no session may
/// write, how a loop's close leaves approved tests out, and the supervision.
pub(crate) struct JudgeEnv<'a> {
    pub(crate) memo: &'a Memo,
    pub(crate) protected: &'a Protected,
    pub(crate) closing: Closing,
    pub(crate) supervision: Supervision<'a>,
}

/// Where the work being judged lives, and what its audit leaves out.
pub(crate) struct Work<'a> {
    /// The checkout the work is in.
    pub(crate) checkout: &'a Path,
    /// The tree the work started from, which its diff is taken against, so
    /// an attempt answers for what an earlier one of its own left behind.
    pub(crate) from: &'a TreeId,
    /// What the adapter staged for its own mechanics, which no audit counts
    /// against the task.
    pub(crate) staged: &'a [PathBuf],
}

/// What a judgement is made on: the work a session left, the scope it answers
/// to, and what settles around it.
pub(crate) struct Sitting<'a> {
    /// The work a session left, in its unit.
    pub(crate) work: Work<'a>,
    /// The task the session works, or `None` for a node's session.
    pub(crate) task: Option<&'a Task>,
    /// The scope before what the request grants: [`scope_in_force`] for a
    /// task, [`crate::node_scope_in_force`] for a node.
    pub(crate) scope: &'a [ScopeGlob],
    /// The request the session made. The caller reads it; judging never
    /// consumes it.
    pub(crate) request: Option<&'a ScopeExpansionRequest>,
    pub(crate) refused: Refusals<'a>,
    /// The window a decision counts against. `None` for a node, and for a
    /// judgement with no request and no refused write to decide.
    pub(crate) window: Option<&'a GrantWindow>,
    /// The limits a request is decided under. `None` exactly when `window` is.
    pub(crate) bounds: Option<Bounds<'a>>,
    /// The criterion the session declared impossible, read from the lock that
    /// holds the declaration.
    pub(crate) declared: Option<&'a CriterionUnpassablePayload>,
    /// The documents a node's session owes its close. `None` for a task.
    pub(crate) documents: Option<Documents<'a>>,
    pub(crate) env: JudgeEnv<'a>,
}

/// What judging a session's work found.
#[derive(Debug, Clone, PartialEq)]
pub struct Judgement {
    /// Every criterion run on the tree as it stands, or answered from the
    /// cache. Empty for a node, which has no criteria.
    pub criteria: Vec<CriterionRun>,
    /// The guards that did not run, by command, because a criterion ahead of
    /// them is red.
    pub waiting: Vec<String>,
    /// Every path the work changed, and those outside its scope.
    pub scope: ScopeCheckResult,
    /// The request the session made, decided on this tree, or the refused
    /// write a red criterion located when no request was made.
    pub request: Option<ScopeExpansionOutcome>,
    /// The documents a node owes its close that the close would refuse.
    pub documents: Vec<DocumentFault>,
    /// The criterion this session declared impossible to pass, if it did.
    pub unpassable: Option<CriterionUnpassablePayload>,
    /// The tree the work stands at, which every answer above is for. `None`
    /// only for a node that declares no scope and owes nothing its close
    /// would refuse: nothing was judged on its tree.
    pub tree: Option<TreeId>,
}

impl Judgement {
    /// Whether the work closes its task or node: criteria pass, nothing waits,
    /// nothing it changed lies outside its scope, and every document is in.
    pub fn closes(&self) -> bool {
        self.criteria.iter().all(|run| run.exit_code == 0)
            && self.waiting.is_empty()
            && self.scope.violations.is_empty()
            && self.documents.is_empty()
    }

    /// Whether a criterion could not run at all, which no change of work alters.
    pub fn unrunnable(&self) -> bool {
        self.criteria
            .iter()
            .any(|run| run.could_not_run().is_some())
    }

    /// Whether a session's stop is held: the work does not close and the close
    /// does not settle it. A grant or an escalation is the close's to decide, an
    /// unrunnable criterion the environment's, and a declared one the planner's.
    pub fn holds_a_stop(&self) -> bool {
        let settles = matches!(
            self.request.as_ref().map(|outcome| &outcome.decision),
            Some(Decision::Grant { .. } | Decision::Escalate { .. })
        );
        !self.closes() && !self.unrunnable() && !settles && self.unpassable.is_none()
    }

    /// A digest of the tree, the decision on the request, the declaration and
    /// the documents the close would refuse. A stop refused for one state is not
    /// refused again while the work stays in it, so a document refused again
    /// with a new body is a different state.
    pub fn state(&self) -> ContentHash {
        let decision = self
            .request
            .as_ref()
            .map(|outcome| DecisionView::of(&outcome.decision));
        // Strings, numbers and enums of those always serialize: no digest is lost.
        let bytes = serde_json::to_vec(&(&self.tree, decision, &self.unpassable, &self.documents))
            .unwrap_or_default();
        ContentHash::sha256(&bytes)
    }
}

/// A decision as the digest reads it: [`Decision`] has no wire form of its own.
#[derive(Serialize)]
enum DecisionView<'a> {
    Grant(&'a [ScopeGlob], &'a Decider),
    Escalate(&'a [ScopeGlob], &'a Waiting),
    Deny(&'a Denial),
}

impl<'a> DecisionView<'a> {
    fn of(decision: &'a Decision) -> Self {
        match decision {
            Decision::Grant { paths, by } => Self::Grant(paths, by),
            Decision::Escalate { paths, because } => Self::Escalate(paths, because),
            Decision::Deny(denial) => Self::Deny(denial),
        }
    }
}

/// The one scope in force for a task's work: its declared `scope:`, then what
/// was granted before the attempt.
pub(crate) fn scope_in_force(task: &Task, grants: &[ScopeGlob]) -> Vec<ScopeGlob> {
    task.scope.iter().chain(grants).cloned().collect()
}

/// The paths an outcome grants: none unless its decision is a grant.
pub(crate) fn paths_granted(outcome: Option<&ScopeExpansionOutcome>) -> &[ScopeGlob] {
    match outcome.map(|outcome| &outcome.decision) {
        Some(Decision::Grant { paths, .. }) => paths,
        _ => &[],
    }
}

/// Judges a session's work: its criteria on the tree, the request decided on
/// that tree, the diff audited against the scope in force plus what the request
/// grants, and the documents a node owes.
pub(crate) async fn judge(
    sitting: Sitting<'_>,
    settle: Settle,
) -> Result<Judgement, TaskCycleError> {
    let PostCheck {
        runs: criteria,
        waiting,
        tree,
    } = post_check_or_tree(&sitting).await?;
    let refused = refused_writes(&sitting, &criteria).await?;
    let request = decide_request(&sitting, &criteria, &refused, settle).await?;
    let ceiling: Vec<ScopeGlob> = sitting
        .scope
        .iter()
        .chain(paths_granted(request.as_ref()))
        .cloned()
        .collect();
    let scope = audit_tree(
        sitting.work.checkout,
        sitting.work.from,
        &tree,
        Ceiling {
            scope: &ceiling,
            protected: sitting.env.protected,
            closing: sitting.env.closing,
        },
        sitting.work.staged,
        sitting.env.supervision,
    )
    .await?;
    let documents = match &sitting.documents {
        Some(documents) => documents.faults().await,
        None => Vec::new(),
    };
    Ok(Judgement {
        criteria,
        waiting,
        scope,
        request,
        documents,
        unpassable: sitting.declared.cloned(),
        tree: Some(tree),
    })
}

/// The criteria the work answers to and the tree they answer for: a task's
/// post-check, or, for a node with no criteria of its own, the tree alone.
async fn post_check_or_tree(sitting: &Sitting<'_>) -> Result<PostCheck, TaskCycleError> {
    let env = &sitting.env;
    match sitting.task {
        Some(task) => post_check(task, sitting.work.checkout, env.memo, env.supervision).await,
        None => Ok(PostCheck {
            runs: Vec::new(),
            waiting: Vec::new(),
            tree: content_of(
                sitting.work.checkout,
                &env.protected.pathspecs(),
                env.supervision,
            )
            .await?,
        }),
    }
}

/// What a decision is made on: the task, the checkout and environment it runs
/// in, and the window and limits it counts against.
pub(super) struct Deciding<'a> {
    pub(super) task: &'a Task,
    pub(super) env: &'a JudgeEnv<'a>,
    pub(super) checkout: &'a Path,
    /// Where a criterion's kept output is read from, when its answer was reused.
    pub(super) kept: Kept<'a>,
    window: &'a GrantWindow,
    bounds: Bounds<'a>,
}

impl Deciding<'_> {
    /// Decides `ask` by the limits and the window this judgement counts against.
    async fn decide(&self, ask: Ask<'_>) -> Result<(Decision, u32), TaskCycleError> {
        crate::scope_expansion::expand(
            ask,
            &self.bounds,
            self.window,
            self.checkout,
            self.env.supervision,
        )
        .await
        .map_err(|source| TaskCycleError::ScopeExpansion {
            task: self.task.id.clone(),
            source,
        })
    }

    /// The outcome of a decision made on located facts, which carry the request
    /// they came from, if any.
    async fn decide_located(
        &self,
        located: Located,
    ) -> Result<ScopeExpansionOutcome, TaskCycleError> {
        let (decision, count) = self
            .decide(Ask::Located {
                paths: &located.paths,
                criterion: &located.criterion,
            })
            .await?;
        Ok(ScopeExpansionOutcome {
            asked: located.asked,
            precheck: located.precheck,
            decision,
            count,
        })
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::ArtifactId;

    use super::*;

    /// A fault names its document and is classified once: a document never
    /// handed over, or a file never produced, is missing; one that is there and
    /// that the close refuses is refused.
    #[test]
    fn a_fault_is_classified_once_as_missing_or_refused() {
        let spec = ArtifactSpec::Opaque("notes.txt".to_string());
        let node = NodeId::from_static("plan");
        let undelivered = ArtifactFailure::Undelivered {
            node: node.clone(),
            artifact: ArtifactId::Opaque {
                name: "notes.txt".to_string(),
            },
        };
        let never_produced = ArtifactFailure::File {
            path: "notes.txt".to_string(),
            problem: FileProblem::Missing { node },
        };
        let empty = ArtifactFailure::File {
            path: "notes.txt".to_string(),
            problem: FileProblem::Empty,
        };
        let refusal = |failure| DocumentFault::of(&spec, failure).refusal;
        assert_eq!(refusal(undelivered), StopRefusal::ArtifactMissing);
        assert_eq!(refusal(never_produced), StopRefusal::ArtifactMissing);
        assert_eq!(refusal(empty), StopRefusal::ArtifactRefused);
    }
}
