//! A loop batch whose session the engine could not stop: the node fails with
//! that fact, once the tasks before the walk's stop are integrated.

use yunta_core::events::{Failure, TokenUsage};
use yunta_core::Node;

use super::dispatch::Dispatched;
use super::integrate::BatchIntegration;
use super::LoopState;
use crate::run::node_close::fail_unstopped;
use crate::run::node_exec::cancelled_end;
use crate::run::{RunCtx, RunError};
use crate::task_cycle::{not_stopped_failure, TaskCycleError};

/// Where the batch walk stopped: at a task whose dispatch failed, or at a task
/// its cancellation interrupted.
pub(super) enum WalkStop {
    /// The walk reached this dispatch's error.
    Failed(RunError),
    /// The walk reached a task that ended interrupted.
    Interrupted,
}

/// Ends the loop node where the batch walk stopped, when a session the engine
/// could not stop or could not confine is among the dispatches the walk has not
/// integrated. The node fails with the first such session's failure and carries
/// every spend the batch holds: the node's own, and what each dispatch the walk
/// has not integrated reported. Those tasks are not integrated: each keeps its
/// status in the log, and a wake of the run runs it again. Without such a session, a
/// walk stopped by a failed dispatch returns that error as it is, and a walk
/// stopped by an interrupted task ends cancelled: by the root cancellation, or
/// failed when a sibling of its join finished first.
pub(super) async fn end_on_unstopped(
    ctx: &RunCtx<'_>,
    node: &Node,
    state: &LoopState,
    stop: WalkStop,
    rest: impl Iterator<Item = Result<Dispatched<'_>, RunError>>,
) -> Result<BatchIntegration, RunError> {
    let mut spent = state.tokens;
    let mut failure: Option<Failure> = None;
    if let WalkStop::Failed(error) = stop {
        let (found, tokens) = unstopped_session(error)?;
        failure = Some(found);
        spent += tokens;
    }
    for dispatch in rest {
        match dispatch {
            Ok(dispatched) => {
                for attempt in &dispatched.report.attempts {
                    spent += attempt.tokens;
                }
            }
            // The walk never reaches a dispatch past its stop, so an error there
            // is not the run's to return; only a session it could not stop or
            // confine counts.
            Err(error) => {
                if let Ok((found, tokens)) = unstopped_session(error) {
                    failure = failure.or(Some(found));
                    spent += tokens;
                }
            }
        }
    }
    match failure {
        Some(failure) => {
            let end = fail_unstopped(ctx, node, failure, spent).await?;
            Ok(BatchIntegration::Ended(end))
        }
        None => Ok(BatchIntegration::Cancelled(cancelled_end(ctx, node).await?)),
    }
}

/// The failure and the spend of a session the engine could not stop or could
/// not confine; any other error is returned as it is. A session refused for its
/// confinement is refused with no token spent, so it carries none.
fn unstopped_session(error: RunError) -> Result<(Failure, TokenUsage), RunError> {
    match error {
        RunError::TaskCycle(TaskCycleError::NotStopped {
            detail,
            release,
            tokens,
            ..
        }) => Ok((not_stopped_failure(&detail, release.as_deref()), tokens)),
        RunError::TaskCycle(TaskCycleError::Confinement { source, .. }) => Ok((
            Failure::Confinement {
                confinement: source,
            },
            TokenUsage::default(),
        )),
        other => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::{TaskId, Unbuildable};

    #[test]
    fn a_session_that_cannot_be_confined_fails_the_node_with_its_cause_and_no_spend() {
        let error = RunError::TaskCycle(TaskCycleError::Confinement {
            task: TaskId::from_static("write-tests"),
            source: Unbuildable::NetworkUnclosable,
        });
        let (failure, spent) =
            unstopped_session(error).expect("a confinement refusal ends the node, not the run");
        assert_eq!(
            failure,
            Failure::Confinement {
                confinement: Unbuildable::NetworkUnclosable,
            }
        );
        assert_eq!(spent, TokenUsage::default());
    }
}
