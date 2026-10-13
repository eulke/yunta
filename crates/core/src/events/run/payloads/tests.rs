use super::*;
use schemars::schema_for;

fn orphan(node: &'static str, on_interrupt: OnInterrupt) -> ResumePolicy {
    ResumePolicy {
        node: NodeId::from_static(node),
        on_interrupt,
    }
}

/// Orphans that resolved alike agree on their policy; orphans that
/// resolved differently, or none at all, agree on none.
#[test]
fn orphans_agree_on_a_policy_only_when_they_resolved_alike() {
    let (restart, fail) = (OnInterrupt::RestartNode, OnInterrupt::FailIfUncertain);
    let alike = RunResumedPayload::new(vec![orphan("a", restart), orphan("b", restart)], None);
    let unlike = RunResumedPayload::new(vec![orphan("a", restart), orphan("b", fail)], None);
    let empty = RunResumedPayload::new(Vec::new(), None);
    assert_eq!(alike.agreed_policy().as_deref(), Some("restart_node"));
    assert_eq!(unlike.agreed_policy(), None);
    assert_eq!(empty.agreed_policy(), None);
}

/// A `run_resumed` an older writer stamped with the policy it agreed on
/// reads back, with that field ignored.
#[test]
fn a_legacy_resume_policy_field_is_ignored() {
    let legacy = serde_json::json!({
        "resume_policy_applied": "restart_node",
        "policies": [{ "node": "build", "on_interrupt": "restart_node" }],
    });
    let read: RunResumedPayload = serde_json::from_value(legacy).unwrap();
    assert_eq!(read.agreed_policy().as_deref(), Some("restart_node"));
}

/// The published shapes carry the pause and the cause, and never the
/// legacy sentences a log may still hold.
#[test]
fn the_published_shapes_leave_the_legacy_sentences_out() {
    let run = serde_json::to_value(schema_for!(RunPausedPayload)).unwrap();
    let promotion = serde_json::to_value(schema_for!(PromotionSignaledPayload)).unwrap();
    let (run, promotion) = (&run["properties"], &promotion["properties"]);
    assert!(run.get("pause").is_some(), "{run}");
    assert!(run.get("reason").is_none(), "{run}");
    assert!(promotion.get("cause").is_some(), "{promotion}");
    assert!(promotion.get("reason").is_none(), "{promotion}");
}

/// A wake that found no tree writes no `tree` key, and one that found a tree
/// reads back as it was written.
#[test]
fn a_wake_that_found_no_tree_writes_no_tree_key() {
    let plain = RunResumedPayload::new(Vec::new(), None);
    let written = serde_json::to_value(&plain).unwrap();
    assert!(written.get("tree").is_none(), "{written}");

    let found = plain.finding("a1a1a1a".parse().unwrap());
    let written = serde_json::to_value(&found).unwrap();
    let read: RunResumedPayload = serde_json::from_value(written).unwrap();
    assert_eq!(read, found);
}
