//! The one map from a scope decision to the run's log: what a session
//! asked, what the engine or a person granted, and what a refusal left —
//! the event that refused it and the finding that says why.
//!
//! Every site that decides a request records through [`record`], so a grant,
//! a refusal and its finding read the same whichever path reached the
//! decision. A request a task closed with, unanswered, is withdrawn through
//! [`withdraw`]: nobody decided it, so it is the module's only other write.

use yunta_core::events::{
    AskedScope, Decider, EventPayload, Finding, FindingCause, FindingEvent, FindingPostedPayload,
    FindingSays, FindingSeverity, OwedScope, ProposedCriterionPrecheck, ScopeAsker, ScopeEvent,
    ScopeExpansionDeniedPayload, ScopeExpansionGrantedPayload, ScopeExpansionRequestedPayload,
    ScopeExpansionWithdrawnPayload, WithdrawalCause,
};
use yunta_core::expansion::{Decision, Denial, Precheck};
use yunta_core::{
    FindingId, Location, NodeId, ProposedCriterionEntry, RelativePath, ScopeExpansionMode,
    ScopeGlob, TaskId,
};

use super::RunError;
use crate::run_log::RunLog;
use crate::scope_expansion::ScopeExpansionRequest;

/// What a decision is about: one task of a loop, or the node itself.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Subject<'a> {
    Task(&'a TaskId),
    Node,
}

impl Subject<'_> {
    /// The task the events name, which a node's events leave out.
    fn task_id(self) -> Option<TaskId> {
        match self {
            Subject::Task(task) => Some(task.clone()),
            Subject::Node => None,
        }
    }
}

/// What a decision was asked: a request a session made, or one a failed loop
/// owes a person's answer to. Its reason is absent when a red criterion of
/// the task located the refused writes, rather than its session.
#[derive(Debug, Clone, Copy)]
pub(crate) enum AskedFor<'a> {
    Request(&'a ScopeExpansionRequest),
    Owed(&'a OwedScope),
}

impl AskedFor<'_> {
    /// The paths the request asks to be allowed.
    fn paths(&self) -> &[ScopeGlob] {
        match self {
            AskedFor::Request(request) => request.paths.as_slice(),
            AskedFor::Owed(owed) => owed.paths.as_slice(),
        }
    }

    /// The session's own reason, when it gave one.
    fn reason(&self) -> Option<&str> {
        match self {
            AskedFor::Request(request) => Some(request.reason.as_str()),
            AskedFor::Owed(owed) => owed.reason.as_deref(),
        }
    }

    /// The criterion a session proposed; an owed request keeps none.
    fn proposed(&self) -> Option<&ProposedCriterionEntry> {
        match self {
            AskedFor::Request(request) => request.proposed_criterion.as_ref(),
            AskedFor::Owed(_) => None,
        }
    }
}

/// One decision and what its caller knows of it, for [`record`].
pub(crate) struct Recording<'a> {
    pub subject: Subject<'a>,
    /// The loop's mode in force. `None` for a node, which has no mode.
    pub mode: Option<ScopeExpansionMode>,
    /// Who decided a refusal: the engine, or a person.
    pub by: Decider,
    /// What the decision was asked. `None` for a grant the engine made from a
    /// red criterion's output, whose request the session never made.
    pub asked: Option<AskedFor<'a>>,
    pub decision: &'a Decision,
    /// The grants the engine has made this run, this one included.
    pub count: u32,
    /// Granted from a refused write the criterion located, not a request.
    pub located: bool,
    /// The precheck of the request's criterion, once the caller has run it.
    pub precheck: Option<Precheck>,
    /// Numbers the refusal's finding, as the attempt that asked does.
    pub attempt: u32,
    /// Whether the request is new to the log. A fresh session that re-reads a
    /// request an earlier attempt logged passes `false`, so it is not logged twice.
    pub new_request: bool,
}

/// Writes what `what` decided onto the run's log.
///
/// A request the session made is recorded first, whatever came of it, unless
/// an earlier attempt already logged it. A grant records the grant. A refusal
/// records the refusal and the finding that says why. An escalation records
/// nothing more: the gate that asks a person records that. A refusal the
/// criterion located records nothing more either: the refused write is
/// recorded where it was found.
pub(crate) async fn record(
    log: &RunLog<'_>,
    node: &NodeId,
    what: Recording<'_>,
) -> Result<(), RunError> {
    // A person answers a request the engine logged when it arrived, so only
    // the engine's own decision writes the request. The refusal's finding
    // reads the session's reason from `what.asked` below either way.
    if let Some(AskedFor::Request(asked)) = what
        .asked
        .filter(|_| what.new_request && !matches!(what.by, Decider::Person { .. }))
    {
        let requested = requested(asked, what.subject.task_id(), what.precheck);
        log.record(
            Some(node),
            EventPayload::Scope(ScopeEvent::Requested(requested)),
        )
        .await?;
    }
    match what.decision {
        Decision::Grant { paths, by } => {
            let granted = ScopeExpansionGrantedPayload {
                task_id: what.subject.task_id(),
                decided_by: by.clone(),
                mode: what.mode,
                count_this_run: what.count,
                paths: paths.clone(),
            };
            log.record(
                Some(node),
                EventPayload::Scope(ScopeEvent::Granted(granted)),
            )
            .await?;
        }
        Decision::Escalate { .. } => {}
        Decision::Deny(_) if what.located => {}
        Decision::Deny(denial) => refuse(log, node, &what, denial).await?,
    }
    Ok(())
}

/// What a task closed with, unanswered: the paths of the request it leaves.
pub(crate) struct Withdrawal<'a> {
    pub task: &'a TaskId,
    pub paths: &'a [ScopeGlob],
}

/// The one writer of a withdrawn request. Nobody decided it, so nothing is
/// granted or denied: the task closed and the request leaves with it.
pub(crate) async fn withdraw(
    log: &RunLog<'_>,
    node: &NodeId,
    request: Withdrawal<'_>,
) -> Result<(), RunError> {
    let withdrawn = ScopeExpansionWithdrawnPayload {
        task_id: request.task.clone(),
        paths: request.paths.to_vec(),
        reason: WithdrawalCause::TaskClosed,
    };
    log.record(
        Some(node),
        EventPayload::Scope(ScopeEvent::Withdrawn(withdrawn)),
    )
    .await?;
    Ok(())
}

/// The request as the session made it: the log keeps it unchanged, with
/// the precheck of its criterion when the caller ran one.
fn requested(
    asked: &ScopeExpansionRequest,
    task_id: Option<TaskId>,
    precheck: Option<Precheck>,
) -> ScopeExpansionRequestedPayload {
    ScopeExpansionRequestedPayload {
        task_id,
        paths: asked.paths.clone(),
        reason: asked.reason.clone(),
        proposed_criterion: asked.proposed_criterion.clone().map(Into::into),
        proposed_criterion_precheck: precheck
            .and_then(Precheck::exit_code)
            .map(|exit_code| ProposedCriterionPrecheck { exit_code }),
    }
}

/// Records a refusal, then the finding it leaves.
async fn refuse(
    log: &RunLog<'_>,
    node: &NodeId,
    what: &Recording<'_>,
    denial: &Denial,
) -> Result<(), RunError> {
    let refused = ScopeExpansionDeniedPayload {
        task_id: what.subject.task_id(),
        decided_by: what.by.clone(),
        mode: what.mode,
        count_this_run: what.count,
        why: Some(denial.clone()),
        legacy_text: None,
    };
    log.record(Some(node), EventPayload::Scope(ScopeEvent::Denied(refused)))
        .await?;
    let finding = denial_finding(node, &what.subject, what.attempt, denial, what.asked)?;
    log.record(
        Some(node),
        EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload { finding })),
    )
    .await?;
    Ok(())
}

/// The finding a refusal leaves: `Minor`, located at the first path asked
/// for, numbered by the attempt, and carrying what was asked, with the
/// session's own reason when it gave one. A refusal is routine control flow,
/// not evidence the run is broken, so it never carries a higher severity.
fn denial_finding(
    node: &NodeId,
    subject: &Subject<'_>,
    attempt: u32,
    denial: &Denial,
    asked: Option<AskedFor<'_>>,
) -> Result<Finding, RunError> {
    let (asker, name) = match subject {
        Subject::Task(task) => (ScopeAsker::Task(TaskId::clone(task)), task.to_string()),
        Subject::Node => (ScopeAsker::Node(node.clone()), node.to_string()),
    };
    let asked = asked.as_ref();
    Ok(Finding {
        id: FindingId::try_from(format!("scope-expansion-{name}-{attempt}"))?,
        severity: FindingSeverity::Minor,
        says: FindingSays::Found {
            cause: FindingCause::ScopeDenied {
                asker,
                denial: denial.clone(),
                asked: asked.map(|asked| AskedScope {
                    paths: asked.paths().to_vec(),
                    reason: asked.reason().map(str::to_string),
                }),
            },
        },
        location: Location::work(
            RelativePath::of(
                asked
                    .and_then(|asked| asked.paths().first())
                    .map(ScopeGlob::as_str),
            ),
            None,
        ),
        proposed_criterion: asked
            .and_then(|asked| asked.proposed())
            .cloned()
            .map(Into::into),
    })
}
