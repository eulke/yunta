//! The claim an escalation states above its record: what the node or the
//! gate says happened or is asked. An engine claim is data, and the words
//! a reader sees are written here; a claim someone wrote is its own words.

use yunta_core::events::{Claim, ExhaustedReroute, FailureMenu};
use yunta_core::{text, NodeId};

/// The claim in the words a reader sees. `menu` is the failure menu the
/// escalation carries: when its re-routes are used up, the node's claim
/// names the re-route it closes.
pub fn sentence(claim: &Claim, menu: Option<&FailureMenu>) -> String {
    match claim {
        Claim::NeedsDecision { node } => format!("gate `{node}` needs a decision"),
        Claim::NodeFailed { node } => match menu.and_then(|menu| menu.reroute.as_ref()) {
            Some(reroute) => exhausted(node, reroute),
            None => format!("node `{node}` failed"),
        },
        Claim::AwaitsReview { node, assignee } => awaits_review(node, assignee.as_deref()),
        Claim::ForgeUnreachable { node, assignee } => forge_unreachable(node, assignee),
        Claim::RecheckUnreachable { node, external_ref } => recheck_unreachable(node, external_ref),
        Claim::Written(said) => said.clone(),
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
        "node `{node}` wants an external gate (assignee: {assignee}) but no forge is reachable from this machine (no credentials, or none configured) — resolve it here instead"
    )
}

/// A published external gate whose review cannot be checked again: its node,
/// the reference its publication left, and the console that answers instead.
fn recheck_unreachable(node: &NodeId, external_ref: &str) -> String {
    format!(
        "node `{node}`'s external gate (published at {external_ref}) needs re-checking, but no forge is reachable from this machine — resolve it here instead"
    )
}

/// A node that failed with its re-routes exhausted: the count of re-routes,
/// the node they went to, and that they are used up.
fn exhausted(node: &NodeId, reroute: &ExhaustedReroute) -> String {
    let max = reroute.max_reroutes as usize;
    format!(
        "node `{node}` failed and its {} to `{}` {} exhausted",
        text::counted(max, "re-route"),
        reroute.goto,
        text::agreeing(max, "is", "are")
    )
}
