//! A node that writes outside its `scope:` fails naming each path, and
//! the decision it asks for offers the one choice that changes the
//! outcome: widening that node's scope by those paths.

use yunta_core::events::{EventPayload, Failure, NodeEvent};
use yunta_engine::current_escalation;
use yunta_testkit::Bench;

mod common;
use common::*;

/// A node allowed only `src/**` that also writes `Cargo.toml`, as a
/// lint fix whose cause lives in a manifest would.
const WRITES_OUTSIDE_WORKFLOW: &str = r#"
name: writes-outside
nodes:
  - id: fix
    kind: bash
    scope: ["src/**"]
    run: "mkdir -p src && echo fixed > src/lib.rs && echo '[lib]' > Cargo.toml"
"#;

/// The failure `node` last recorded.
fn last_failure(bench: &Bench, node: &str) -> Failure {
    bench
        .events()
        .iter()
        .rev()
        .filter(|e| e.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .find_map(|e| match e.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(p.failure.clone()),
            _ => None,
        })
        .expect("the node failed")
}

#[tokio::test]
async fn a_scope_violation_names_every_path_outside_and_the_decision_shows_them() {
    let bench = parked(WRITES_OUTSIDE_WORKFLOW, "sessions: []\n").await;

    let failure = last_failure(&bench, "fix");
    assert_eq!(
        failure.outside_scope(),
        [std::path::PathBuf::from("Cargo.toml")],
        "the failure is the list of paths, not a count: {failure}"
    );

    let (_, escalation) =
        current_escalation(&bench.manifest(), &yunta_engine::derive(&bench.events()))
            .expect("a failed node is a pause with a menu");
    let evidence = escalation.evidence().lines().join("\n");
    assert!(
        evidence.contains("Cargo.toml"),
        "the person deciding reads which file fell outside: {evidence}"
    );
}
