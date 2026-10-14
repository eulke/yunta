//! The words of a gate a person decided, and of a review or a plan that
//! stopped a run: a choice that routes the run back, a request the console
//! rejected, a plan sent back unpublished, findings open on a gate, a
//! request for changes and a closed pull request.

use yunta_core::events::{Approval, Failure};
use yunta_core::shown::{withheld_because, Flaw};
use yunta_core::{text, FindingId, NodeId, OptionId, TaskId};

/// The line a choice that routes the run back states: which option, and the
/// node it routes to.
pub(super) fn gate_chose_headline(option: &OptionId, target: &NodeId) -> String {
    format!("gate chose `{option}` — re-routing to `{target}`")
}

/// The choice a person made and wrote, under the line that states it.
pub(super) fn gate_chose(failure: &Failure) -> String {
    let Failure::GateChose {
        option,
        target,
        said,
        ..
    } = failure
    else {
        unreachable!("routed here only for a gate's choice")
    };
    text::detailed(
        gate_chose_headline(option, target),
        said.as_deref().unwrap_or_default(),
    )
}

/// The cause a re-route carries for a gate's choice: which gate chose what,
/// and the words the person gave with it.
pub(super) fn gate_choice_cause(failure: &Failure) -> String {
    let Failure::GateChose {
        node, option, said, ..
    } = failure
    else {
        unreachable!("routed here only for a gate's choice")
    };
    text::detailed(
        format!("gate `{node}` chose `{option}`"),
        said.as_deref().unwrap_or_default(),
    )
}

/// A person accepted that a task's tests are wrong: the node that owes them
/// again runs ahead of the loop.
pub(super) fn tests_accepted(task: &TaskId, owed_by: &NodeId) -> String {
    format!(
        "a person accepted that task `{task}`'s tests are wrong — `{owed_by}` writes them again \
         before the loop goes on"
    )
}

/// A gate a person rejected, by the place the approval was given.
pub(super) fn rejected(on: &Approval) -> String {
    match on {
        Approval::Console => "rejected from the console".to_string(),
        Approval::Review | Approval::Merge => "rejected on the forge".to_string(),
    }
}

/// A plan that is not published and goes back for a rewrite, because of
/// the flaws it has.
pub(super) fn sent_back(flaws: &[Flaw]) -> String {
    let because = withheld_because(flaws).unwrap_or_default();
    format!("not published, and sent back: {because}")
}

/// A findings gate that holds findings at or above its severity: how
/// many, the severity, and the ids of the findings.
pub(super) fn findings_open(failure: &Failure) -> String {
    let Failure::FindingsOpen {
        at_or_above,
        open_findings,
    } = failure
    else {
        unreachable!("routed here only for a findings failure")
    };
    let ids: Vec<&str> = open_findings.iter().map(FindingId::as_str).collect();
    format!(
        "{} at or above {}: {}",
        text::counted(open_findings.len(), "finding"),
        at_or_above.as_str(),
        ids.join(", ")
    )
}

/// A reviewer who asked for changes on the reviewed commit: who, which
/// commit, and how many comments they left.
pub(super) fn changes_requested(failure: &Failure) -> String {
    let Failure::ChangesRequested {
        requested_by,
        reviewed_sha,
        comments,
    } = failure
    else {
        unreachable!("routed here only for a changes-requested failure")
    };
    format!(
        "changes requested by {requested_by} at {reviewed_sha} ({})",
        text::counted(*comments, "comment")
    )
}

/// A pull request closed without an approval, named by its reference when
/// the failure knows it.
pub(super) fn pull_request_closed(failure: &Failure) -> String {
    let Failure::PullRequestClosed { .. } = failure else {
        unreachable!("routed here only for a closed pull request")
    };
    "the pull request was closed without approval".to_string()
}
