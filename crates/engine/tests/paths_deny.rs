//! What the project denies to every run (`permissions.paths.deny`) no
//! node with a checkout of its own lands, no task integrates, no request
//! widens and no person is offered.

use yunta_core::events::{EventPayload, Failure, NodeEvent, ScopeEvent, StoredEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git_output, Bench, MOCK_CONFIG};

mod common;
use common::{plan_session, scope_expansion_workflow, task_yaml};

fn denying_ci() -> String {
    format!("{MOCK_CONFIG}permissions:\n  paths:\n    deny: [\".github/**\"]\n")
}

fn failures(events: &[StoredEvent], node: &str) -> Vec<Failure> {
    events
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(p.failure.clone()),
            _ => None,
        })
        .collect()
}

fn denied_in_scope_checks(events: &[StoredEvent]) -> Vec<std::path::PathBuf> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::ScopeChecked(p))) => Some(p.denied.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// A node whose scope admits the file still may not write what the
/// project denies: it fails with no grant to offer, and its checkout
/// never lands.
#[tokio::test]
async fn a_scoped_node_writing_a_denied_path_fails_without_a_grant_and_never_lands() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - { id: tidy, kind: bash, scope: [\"**\"], run: \"mkdir -p .github && echo x > .github/ci.yml && echo y > notes.txt\" }\n";
    let RunReport { terminal, .. } = bench
        .run_with_config(workflow, "sessions: []\n", &denying_ci())
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let events = bench.events();
    let failed = failures(&events, "tidy");
    assert_eq!(
        failed,
        vec![Failure::paths_denied(vec![".github/ci.yml".into()])]
    );
    assert!(!failed[0].wants_scope(), "no grant widens a deny");
    assert_eq!(
        denied_in_scope_checks(&events),
        [".github/ci.yml"].map(std::path::PathBuf::from)
    );
    let landed = git_output(&bench.worktree, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(
        !landed.contains(".github/ci.yml") && !landed.contains("notes.txt"),
        "{landed}"
    );
}

/// The planner's plan and one task scoped to `scope` whose session writes
/// `writes`, under an `ask` expansion mode.
fn one_task(scope: &str, writes: &str, request: Option<&str>) -> (String, String) {
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-w", "w", scope, "test -f a.txt")
    );
    let mut fixture = plan_session(&tasks);
    let request = request
        .map(|paths| {
            format!(
                "      - {{ path: {:?}, content: {:?} }}\n",
                yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
                format!("paths: [{paths}]\nreason: \"the workflow file is wrong\"\n"),
            )
        })
        .unwrap_or_default();
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-w\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: {writes:?}, content: \"x\" }}\n{request}    outcome: {{ type: completed, summary: did-w }}\n"
    ));
    (
        scope_expansion_workflow("rules", &[".github/**"], None),
        fixture,
    )
}

#[tokio::test]
async fn a_task_writing_a_denied_path_is_rejected() {
    let bench = Bench::new();
    let (workflow, fixture) = one_task("**", ".github/ci.yml", None);
    bench
        .run_with_config(&workflow, &fixture, &denying_ci())
        .await;

    let events = bench.events();
    assert!(
        denied_in_scope_checks(&events).contains(&".github/ci.yml".into()),
        "the scope check names what the project denies"
    );
    let landed = git_output(&bench.worktree, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(!landed.contains(".github/ci.yml"), "{landed}");
}

/// Under `rules` whose `within` would admit it, a request for a denied
/// path is still refused by rule, without asking anyone.
#[tokio::test]
async fn a_task_request_for_a_denied_path_is_refused_by_rule_without_asking() {
    let bench = Bench::new();
    let (workflow, fixture) = one_task("a.txt", "a.txt", Some(".github/ci.yml"));
    bench
        .run_with_config(&workflow, &fixture, &denying_ci())
        .await;

    let reasons: Vec<String> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Denied(p))) => p.denial_reason.clone(),
            _ => None,
        })
        .collect();
    assert!(
        reasons
            .iter()
            .any(|reason| reason.contains("the project denies it to every run")),
        "{reasons:?}"
    );
    assert!(!bench.events().iter().any(|event| matches!(
        event.payload(),
        Some(EventPayload::Scope(ScopeEvent::Granted(_)))
    )));
}

/// A node's session that asks for a denied path is never put to a
/// person: nothing else was asked, so its close goes on.
#[tokio::test]
async fn a_nodes_request_for_a_denied_path_is_never_put_to_a_person() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - { id: fix, kind: prompt, runner: executor, prompt: fix, scope: [\"src/**\"] }\n";
    let fixture = "capabilities: { run_tools: true }\nsessions:\n  - steps:\n      - type: run_tool\n        tool: yunta_request_scope_expansion\n        arguments: { paths: [.github/ci.yml], reason: \"the workflow is wrong\" }\n    outcome: { type: completed, summary: asked }\n";
    let RunReport { terminal, .. } = bench
        .run_with_config(workflow, fixture, &denying_ci())
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(failures(&bench.events(), "fix").is_empty());
}
