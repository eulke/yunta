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

    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
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
async fn menu(bench: &Bench) -> Vec<String> {
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
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

    assert_eq!(menu(&bench).await, ["grant", "retry", "abort"]);
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
    .unwrap();
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
    assert_eq!(menu(&bench).await, ["retry", "abort"]);
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

/// A prompt node allowed only `src/**`, whose first session asks for the
/// manifest instead of writing it, and whose second writes it.
const ASKS_FIRST_WORKFLOW: &str = r#"
name: asks-first
nodes:
  - id: fix
    kind: prompt
    runner: executor
    prompt: "Fix the lint."
    scope: ["src/**"]
"#;

const ASKS_FIRST_FIXTURE: &str = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_request_scope_expansion
        arguments: { paths: [Cargo.toml], reason: "the lint's cause is the manifest" }
    outcome: { type: completed, summary: "asked" }
  - effects:
      - { path: Cargo.toml, content: "[lib]" }
    outcome: { type: completed, summary: "fixed" }
"#;

#[tokio::test]
async fn a_node_session_that_asks_for_scope_puts_its_request_to_a_person() {
    let bench = parked(ASKS_FIRST_WORKFLOW, ASKS_FIRST_FIXTURE).await;

    let failure = last_failure(&bench, "fix");
    assert!(
        matches!(failure, Failure::ScopeRequested { .. }),
        "the node's work waits on the answer: {failure}"
    );
    assert_eq!(menu(&bench).await, ["grant", "retry", "abort"]);
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
    .unwrap();
    let evidence = escalation.evidence().lines().join("\n");
    assert!(
        evidence.contains("the lint's cause is the manifest"),
        "the person reads the session's own reason: {evidence}"
    );
    let requested = events_matching(
        &bench,
        |p| matches!(p, EventPayload::Scope(ScopeEvent::Requested(r)) if r.task_id.is_none()),
    );
    assert_eq!(requested, 1, "the request is on the log as the node's own");
    assert_eq!(
        bench.mock().requests_seen()[0].fence.advice,
        yunta_core::fence::Advice::RequestExpansion,
        "a node's session that can ask is told to"
    );
}

#[tokio::test]
async fn a_granted_request_lets_the_next_attempt_write_what_was_asked_for() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(
            ASKS_FIRST_WORKFLOW,
            ASKS_FIRST_FIXTURE,
            &SequencedInteraction::choosing(&["grant"]),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let grants = grants(&bench);
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].1.paths, ["Cargo.toml"]);
}

#[tokio::test]
async fn under_a_ceiling_that_denies_expansions_a_node_session_is_not_offered_the_request() {
    let bench = Bench::new();
    let config = format!("{MOCK_CONFIG}permissions:\n  scope_expansion:\n    max_mode: deny\n");
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_request_scope_expansion
        arguments: { paths: [Cargo.toml], reason: "the lint's cause is the manifest" }
        expect: refused
    outcome: { type: completed, summary: "stayed inside" }
"#;
    let RunReport { terminal, .. } = bench
        .run_with_config(ASKS_FIRST_WORKFLOW, fixture, &config)
        .await;

    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "nothing was written outside"
    );
    assert_eq!(
        bench.mock().requests_seen()[0].fence.advice,
        yunta_core::fence::Advice::ReportFinding
    );
}

/// The session that asked for the manifest is the one that writes it:
/// picked back up in the checkout it saw, told what was granted and which
/// tool shows its scope, rather than a fresh session reading the brief.
#[tokio::test]
async fn a_granted_request_resumes_the_node_session_that_asked() {
    let fixture = ASKS_FIRST_FIXTURE.replace(
        "capabilities: { run_tools: true }",
        "capabilities: { run_tools: true, resume_session: true }",
    );
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(
            ASKS_FIRST_WORKFLOW,
            &fixture,
            &SequencedInteraction::choosing(&["grant"]),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let opened: Vec<_> = bench
        .events()
        .iter()
        .filter_map(|e| match e.payload() {
            Some(EventPayload::Session(yunta_core::events::SessionEvent::Opened(p))) => {
                Some(p.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(opened.len(), 2, "{opened:?}");
    assert_eq!(opened[1].continues.as_ref(), Some(&opened[0].session_id));
    assert_eq!(
        bench.mock().resumes_seen(),
        vec![opened[0].session_id.clone()]
    );
    let resumed = bench.mock().requests_seen()[1].clone();
    assert!(
        resumed.cwd.ends_with("unit-worktrees/node/fix-1"),
        "{}",
        resumed.cwd.display()
    );
    assert!(
        resumed.prompt.contains("was granted: Cargo.toml")
            && resumed.prompt.contains("yunta_check_scope"),
        "{}",
        resumed.prompt
    );
}
