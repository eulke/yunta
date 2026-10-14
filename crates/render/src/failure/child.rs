//! The words of the children a node waits on: a parallel group that failed
//! under its join, a child that ran with no terminal event when the engine
//! last stopped, the loser of a join-any race, and a composed workflow that
//! could not start, nested too deep, or whose run failed.

use yunta_core::events::{ChildRefusal, Failure, Interruption};
use yunta_core::{text, JoinPolicy, NodeId, RunId};

use super::headline;
use crate::inputs;

/// A composed workflow whose run ended in failure: the child run, and what
/// failed it under it when the child's log says.
pub(super) fn ended(failure: &Failure) -> String {
    let Failure::ChildFailed {
        child_run,
        child_failure,
    } = failure
    else {
        unreachable!("routed here only for a failed child run")
    };
    match child_failure.as_deref() {
        Some(child) => format!(
            "child run `{child_run}` failed:\n  {}",
            text::hanging(&super::sentence(child), "  ")
        ),
        None => format!("child run `{child_run}` failed"),
    }
}

/// The headline of a child run that failed: its run, and what failed it.
pub(super) fn failed_headline(child_run: &RunId, child: &Failure) -> String {
    format!("child run `{child_run}` failed: {}", headline(child))
}

/// A parallel group that failed under its join. Under `all`, the first failed
/// child is named; under `any`, no child succeeded, so every child failed.
pub(super) fn children_failed(failure: &Failure) -> String {
    let Failure::ChildrenFailed {
        join,
        children_failed,
    } = failure
    else {
        unreachable!("routed here only for a failed group")
    };
    match join {
        JoinPolicy::All => match children_failed.first() {
            Some(first) => format!("child `{first}` failed under join: all"),
            None => "a child failed under join: all".to_string(),
        },
        JoinPolicy::Any => format!(
            "join: any — no child succeeded ({} failed)",
            children_failed.len()
        ),
    }
}

/// A child that ran with no terminal event when the engine last stopped, and
/// that its `on_interrupt` policy will not let run again.
pub(super) fn uncertain(node: &NodeId) -> String {
    format!(
        "`{node}` was running with no terminal event when the engine last stopped — \
         `on_interrupt: fail_if_uncertain` refuses to guess whether it finished; verify manually \
         before resuming"
    )
}

/// The loser of a join-any race: a sibling in the group finished first.
pub(super) fn interrupted(failure: &Failure) -> String {
    let Failure::Interrupted { interrupted_by } = failure else {
        unreachable!("routed here only for an interrupted node")
    };
    match interrupted_by {
        Interruption::SiblingWon => {
            "interrupted: a sibling in this join: any group finished first".to_string()
        }
    }
}

/// A composed workflow the run could not start, by the reason it was refused.
pub(super) fn refused(workflow: &str, refusal: &ChildRefusal) -> String {
    match refusal {
        ChildRefusal::Unresolved { problem } => problem.clone(),
        ChildRefusal::Invalid { report } => report.to_string(),
        ChildRefusal::FailsCheck { rules } => fails_check(workflow, rules),
        ChildRefusal::Inputs { problem } => inputs::problem(problem),
    }
}

/// A composed workflow that fails its own check: the use, and the codes of the
/// rules it refused.
pub(super) fn fails_check(workflow: &str, rules: &[String]) -> String {
    format!("`use: {workflow}` fails check: {}", rules.join("; "))
}

/// A composed workflow that would nest past the depth the run allows: the
/// child's name when the failure carries it, the level it would reach, and
/// the limit.
pub(super) fn nested_too_deep(failure: &Failure) -> String {
    let Failure::NestedTooDeep {
        workflow,
        depth,
        max_depth,
    } = failure
    else {
        unreachable!("routed here only for a nested workflow")
    };
    let child = match workflow.is_empty() {
        true => "a child workflow".to_string(),
        false => format!("child workflow `{workflow}`"),
    };
    format!(
        "creating {child} would nest {} deep but `limits.max_workflow_depth` is {max_depth} — \
         flatten the composition or raise the limit",
        text::counted(*depth as usize, "level")
    )
}
