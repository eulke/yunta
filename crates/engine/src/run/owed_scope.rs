//! A person's answer to a loop that ended owing scope decisions: what its
//! tasks asked to be allowed to write, which nobody was there to decide
//! while the loop ran.
//!
//! The answer is the failure's: `grant` widens each task's scope by what it
//! asked for, when the protected set allows every one of those paths, and
//! the task's next attempt picks its work back up with the wider scope. Any
//! other answer denies the request, with the choice as its reason, and the
//! tasks go on within what they declared, however the answer runs the loop
//! again.

use yunta_core::events::{Decider, EventPayload, Failure, OwedScope, ScopeEvent, StoredEvent};
use yunta_core::expansion::Decision;
use yunta_core::{Node, Seq};

use super::scope_record::{self, AskedFor, Recording, Subject};
use super::{RunCtx, RunError};
use crate::replay::NodeState;
use crate::reserved::decision_of_choice;

/// Answers the scope requests `node`'s loop ended owing, when a person
/// answered that failure: every request, with that answer. Once per
/// decision: a request the log already answered after it is left alone.
pub(super) async fn answer(ctx: &RunCtx<'_>, node: &Node) -> Result<(), RunError> {
    let view = ctx.run_view().await?;
    let Some((decided_at, choice)) = view.state.choice_after_failure(&node.id) else {
        return Ok(());
    };
    let Some(NodeState::Failed {
        failure: Failure::ScopeOwed { owed },
        ..
    }) = view.state.nodes.state(&node.id)
    else {
        return Ok(());
    };
    let attempt = finding_number(decided_at)?;
    let by = Decider::Person {
        id: choice.by.clone(),
    };
    let protected = ctx.protected();
    let log = ctx.log();
    let unanswered = owed
        .iter()
        .filter(|request| !answered_since(&view.events, request, decided_at));
    for request in unanswered {
        let decision = decision_of_choice(choice, &request.paths, &protected, &by);
        // A grant has no finding to word. A refusal's finding keeps the
        // session's own reason, so the request it answers is passed for it.
        let asked = match &decision {
            Decision::Deny(_) => Some(AskedFor::Owed(request)),
            Decision::Grant { .. } | Decision::Escalate { .. } => None,
        };
        scope_record::record(
            &log,
            &node.id,
            Recording {
                subject: Subject::Task(&request.task_id),
                mode: Some(super::loop_exec::mode_in_force(node)),
                by: by.clone(),
                asked,
                decision: &decision,
                count: view.state.grants.granted(),
                located: false,
                precheck: None,
                attempt,
                new_request: true,
            },
        )
        .await?;
    }
    Ok(())
}

/// The number an answer's finding takes: the answer's own position on the log,
/// which no other answer of the run shares.
fn finding_number(decided_at: Seq) -> Result<u32, RunError> {
    u32::try_from(decided_at.get()).map_err(|_| RunError::Broken {
        diagnostic: format!("the answer at event {decided_at} is past what a finding numbers"),
    })
}

/// Whether the log answered `request` after `decided_at`.
fn answered_since(events: &[StoredEvent], request: &OwedScope, decided_at: Seq) -> bool {
    events
        .iter()
        .filter(|event| event.seq > decided_at)
        .any(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Granted(granted))) => {
                granted.task_id.as_ref() == Some(&request.task_id)
            }
            Some(EventPayload::Scope(ScopeEvent::Denied(denied))) => {
                denied.task_id.as_ref() == Some(&request.task_id)
            }
            _ => false,
        })
}
