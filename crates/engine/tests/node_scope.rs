//! A node that writes outside its `scope:` fails naming each path, and
//! the decision it asks for offers the one choice that changes the
//! outcome: widening that node's scope by those paths.

use yunta_core::events::{
    EventDraft, EventPayload, Failure, NodeEvent, ScopeEvent, ScopeExpansionGrantedPayload,
};
use yunta_core::Clock;
use yunta_engine::{current_escalation, RunReport, RunTerminal};
use yunta_testkit::{Bench, MOCK_CONFIG};

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

/// Every grant on the log: the node it was written under, the task it
/// names, and the paths it added.
fn grants(bench: &Bench) -> Vec<(Option<String>, ScopeExpansionGrantedPayload)> {
    bench
        .events()
        .iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::Scope(ScopeEvent::Granted(p))) => {
                Some((e.node_id.as_ref().map(|id| id.to_string()), p.clone()))
            }
            _ => None,
        })
        .collect()
}

/// How many attempts of `node` the run started.
fn attempts(bench: &Bench, node: &str) -> usize {
    bench
        .events()
        .iter()
        .filter(|e| {
            e.node_id.as_ref().is_some_and(|id| id.as_str() == node)
                && matches!(e.payload(), Some(EventPayload::Node(NodeEvent::Started(_))))
        })
        .count()
}

/// The option ids the decision a parked bench waits on offers.
fn menu(bench: &Bench) -> Vec<String> {
    let (_, escalation) =
        current_escalation(&bench.manifest(), &yunta_engine::derive(&bench.events()))
            .expect("a failed node is a pause with a menu");
    escalation
        .options()
        .iter()
        .map(|option| option.id.to_string())
        .collect()
}

#[tokio::test]
async fn a_scope_violation_is_offered_the_grant_that_widens_the_node_by_what_it_wrote() {
    let bench = parked(WRITES_OUTSIDE_WORKFLOW, "sessions: []\n").await;

    assert_eq!(menu(&bench), ["grant", "retry", "abort"]);
    let (_, escalation) =
        current_escalation(&bench.manifest(), &yunta_engine::derive(&bench.events())).unwrap();
    assert_eq!(
        escalation.options()[0].label,
        "Allow `fix` to also write Cargo.toml and run it again (attempt 2)"
    );
}

#[tokio::test]
async fn a_grant_chosen_while_parked_widens_the_scope_once_and_the_next_attempt_passes() {
    let bench = parked(WRITES_OUTSIDE_WORKFLOW, "sessions: []\n").await;

    answer_parked(&bench, "grant").await.unwrap();
    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(attempts(&bench, "fix"), 2);
    let grants = grants(&bench);
    assert_eq!(grants.len(), 1, "one decision, one grant: {grants:?}");
    let (node, grant) = &grants[0];
    assert_eq!(node.as_deref(), Some("fix"));
    assert_eq!(grant.task_id, None, "the node's own scope, not a task's");
    assert_eq!(grant.paths, ["Cargo.toml"]);
}

#[tokio::test]
async fn a_grant_chosen_while_asked_continues_the_same_invocation() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(
            WRITES_OUTSIDE_WORKFLOW,
            "sessions: []\n",
            &SequencedInteraction::choosing(&["grant"]),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(grants(&bench).len(), 1);
}

#[tokio::test]
async fn a_restart_after_the_grant_never_grants_it_twice() {
    // A crash between the grant and the attempt it widens: the next wake
    // finds the grant already after the decision and starts the attempt.
    let bench = parked(WRITES_OUTSIDE_WORKFLOW, "sessions: []\n").await;
    answer_parked(&bench, "grant").await.unwrap();
    bench
        .storage
        .async_handle()
        .append(
            EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("fix".into()),
                payload: EventPayload::Scope(ScopeEvent::Granted(ScopeExpansionGrantedPayload {
                    task_id: None,
                    decided_by: yunta_core::events::Decider::Person { id: "mcp".into() },
                    mode: yunta_core::ScopeExpansionMode::Ask,
                    count_this_run: 1,
                    paths: vec!["Cargo.toml".into()],
                })),
            },
            yunta_testkit_core::FixedClock.now(),
        )
        .await
        .unwrap();

    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(grants(&bench).len(), 1);
}

#[tokio::test]
async fn a_ceiling_that_denies_every_expansion_leaves_retry_and_abort() {
    let bench = Bench::new();
    let config = format!("{MOCK_CONFIG}permissions:\n  scope_expansion:\n    max_mode: deny\n");
    let RunReport { terminal, .. } = bench
        .run_with_config(WRITES_OUTSIDE_WORKFLOW, "sessions: []\n", &config)
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(menu(&bench), ["retry", "abort"]);
}

#[tokio::test]
async fn the_attempt_after_a_grant_is_fenced_to_the_widened_scope() {
    let workflow = r#"
name: prompt-writes-outside
nodes:
  - id: fix
    kind: prompt
    runner: executor
    prompt: "Fix the lint."
    scope: ["src/**"]
"#;
    let fixture = r#"
sessions:
  - effects:
      - { path: Cargo.toml, content: "[lib]" }
    outcome: { type: completed, summary: "fixed" }
  - effects:
      - { path: Cargo.toml, content: "[lib]" }
    outcome: { type: completed, summary: "fixed" }
"#;
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(
            workflow,
            fixture,
            &SequencedInteraction::choosing(&["grant"]),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let requests = bench.mock().requests_seen();
    assert_eq!(
        requests[0].fence.allowed.as_deref(),
        Some(["src/**".into()].as_slice())
    );
    assert_eq!(
        requests[1].fence.allowed.as_deref(),
        Some(["src/**".into(), "Cargo.toml".into()].as_slice()),
        "the session is held to what the node declared and what a person granted it"
    );
}
