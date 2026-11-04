//! The claim an escalation states above its record: what the node or the
//! gate says happened or is asked. An engine claim is data, and the words
//! a reader sees are written here; a claim someone wrote is its own words,
//! said on one line.

use surco::text;
use yunta_core::events::{Claim, ExhaustedReroute, FailureMenu};
use yunta_core::NodeId;

use crate::found;

/// The claim in the words a reader sees. `menu` is the failure menu the
/// escalation carries: when its reroutes are used up, the node's claim
/// names the reroute it closes.
pub fn sentence(claim: &Claim, menu: Option<&FailureMenu>) -> String {
    match claim {
        Claim::NeedsDecision { node } => format!("gate `{node}` needs a decision"),
        Claim::ApprovePlan { summary } => format!("Approve the plan: {}?", change(summary)),
        Claim::NodeFailed { node } => match menu.and_then(|menu| menu.reroute.as_ref()) {
            Some(reroute) => exhausted(node, reroute),
            None => format!("node `{node}` failed"),
        },
        Claim::AwaitsReview { node, assignee } => awaits_review(node, assignee.as_deref()),
        Claim::ForgeUnreachable { node, assignee } => forge_unreachable(node, assignee),
        Claim::RecheckUnreachable { node, external_ref } => recheck_unreachable(node, external_ref),
        Claim::TokensSpent { run } => format!("run `{run}` exhausted its token budget"),
        Claim::LoopCapSpent { node } => {
            format!("loop `{node}` needs another iteration and its cap is spent")
        }
        Claim::ScopeAsked { task, reason } => format!(
            "task `{task}` requests scope expansion: {}",
            found::stated_reason(reason.as_deref())
        ),
        Claim::Departs { task } => format!("task `{task}`'s session departs from the plan"),
        Claim::Written(said) => text::one_line(said),
    }
}

/// A gate waiting on an external review: its node, and the assignee who
/// gives the review, when the claim names one.
fn awaits_review(node: &NodeId, assignee: Option<&str>) -> String {
    match assignee {
        Some(assignee) => {
            format!("node `{node}` is waiting on external review (assignee: {assignee})")
        }
        None => format!("node `{node}` is waiting on external review"),
    }
}

/// An external gate whose pull request cannot be published: its node, the
/// assignee the gate names, and the console that answers in its place.
fn forge_unreachable(node: &NodeId, assignee: &str) -> String {
    format!(
        "node `{node}` wants an external gate (assignee: {assignee}) but no forge is reachable from this machine (no credentials, or none configured): resolve it here instead"
    )
}

/// A published external gate whose review cannot be checked again: its node,
/// the reference its publication left, and the console that answers instead.
fn recheck_unreachable(node: &NodeId, external_ref: &str) -> String {
    format!(
        "node `{node}`'s external gate (published at {external_ref}) needs re-checking, but no forge is reachable from this machine: resolve it here instead"
    )
}

/// A node that failed with its reroutes exhausted: the count of reroutes,
/// the node they went to, and that they are used up.
fn exhausted(node: &NodeId, reroute: &ExhaustedReroute) -> String {
    let max = reroute.max_reroutes as usize;
    format!(
        "node `{node}` failed and its {} to `{}` {} exhausted",
        text::counted(max, "reroute"),
        reroute.goto,
        text::agreeing(max, "is", "are")
    )
}

/// The change a plan's summary names, as a question or a title says it: on
/// one line, trimmed, with the stops it ends on taken off, so the sentence
/// that quotes it ends it.
pub(crate) fn change(summary: &str) -> String {
    text::one_line(summary)
        .trim()
        .trim_end_matches(['.', '!', '?'])
        .trim_end()
        .to_string()
}
