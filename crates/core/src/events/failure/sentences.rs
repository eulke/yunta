//! The sentences a node's failure and a loop's cap end a run with. The
//! engine writes them into a failure's claim, so they live here beside the
//! failure's own words; render says the same sentences for a parked run.

use crate::ids::NodeId;

use super::Failure;

/// The half every cap's sentence ends with: a reader who hit a declared
/// limit has the same two ways past it whichever limit it was.
pub const PAST_THE_CAP: &str =
    "resume with an interactive surface to continue past the cap or abort";

/// The failure a loop node takes when its iteration cap stops it and no
/// surface lifts the cap: the loop, the limit and the two ways past it.
/// The node keeps these words as its failure, so the parked run names the
/// limit to whichever surface reads it.
pub fn loop_overrun_failure(node: &NodeId, cap: u32) -> String {
    format!("loop `{node}` exceeded `limits.max_loop_iterations` ({cap}) — {PAST_THE_CAP}")
}

/// The line a run closes with when a failed node ends it: the node, and the
/// headline of what failed it. What a command printed stays in the failure's
/// detail, not inside this line.
pub fn node_failed_line(node: &NodeId, failure: &Failure) -> String {
    format!("node `{node}` failed: {}", failure.headline())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_denial_read_from_a_log_without_reasons_names_its_paths() {
        let failure: Failure =
            serde_json::from_value(serde_json::json!({"denied_paths": ["a.txt", ".claude/x"]}))
                .expect("a log written before reasons existed reads back");
        assert_eq!(
            failure,
            Failure::PathsDenied {
                denied_paths: vec!["a.txt".into(), ".claude/x".into()],
                reasons: Vec::new(),
            }
        );
        assert_eq!(
            failure.to_string(),
            "wrote what no session of the run may write — a.txt, .claude/x: undo those changes"
        );
    }
}
