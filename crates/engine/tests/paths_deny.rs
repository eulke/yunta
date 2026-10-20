//! What the project denies to every run (`permissions.paths.deny`) no
//! node with a checkout of its own lands, no task integrates, no request
//! widens and no person is offered.

use yunta_core::events::{
    EventPayload, Failure, NodeEvent, ScopeEvent, SessionEvent, StoredEvent, ToolTarget,
};
use yunta_core::expansion::Denial;
use yunta_core::protected::Protection;
use yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, git_output, Bench, MOCK_CONFIG};

mod common;
use common::{plan_session, request_step, scope_expansion_workflow, task_yaml};

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
        .run_with_config(
            workflow,
            "dialect: claude-code\nsessions: []\n",
            &denying_ci(),
        )
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let events = bench.events();
    let failed = failures(&events, "tidy");
    assert_eq!(
        failed,
        vec![Failure::paths_denied(
            vec![".github/ci.yml".into()],
            vec![Protection::ProjectDeny]
        )]
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

/// What an agent's CLI loads its own settings, hooks and MCP servers from
/// no node writes, whichever CLI it runs on: a pooled checkout keeps the
/// file for the next session that opens there, and what it says would run
/// outside that session's sandbox.
#[tokio::test]
async fn a_node_writing_an_agent_s_own_config_fails_without_a_grant_and_never_lands() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - { id: tidy, kind: bash, scope: [\"**\"], run: \"mkdir -p .codex && echo x > .codex/hooks.json && echo y > notes.txt\" }\n";
    let fixture = "dialect: codex\nsessions: []\n";
    let RunReport { terminal, .. } = bench.run_with_config(workflow, fixture, MOCK_CONFIG).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let failed = failures(&bench.events(), "tidy");
    assert_eq!(
        failed,
        vec![Failure::paths_denied(
            vec![".codex/hooks.json".into()],
            vec![Protection::AgentConfig]
        )]
    );
    assert!(!failed[0].wants_scope(), "no grant widens it");
    let landed = git_output(&bench.worktree, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(!landed.contains(".codex/hooks.json"), "{landed}");
}

/// Every agent CLI's configuration is protected, not only the configuration
/// of the CLIs a run drives: a run on Claude Code alone still refuses a
/// write to what Codex reads.
#[tokio::test]
async fn a_run_on_one_cli_refuses_a_write_to_another_cli_s_config() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - { id: tidy, kind: bash, scope: [\"**\"], run: \"mkdir -p .codex && echo x > .codex/hooks.json\" }\n";
    let fixture = "dialect: claude-code\nsessions: []\n";
    bench.run_with_config(workflow, fixture, MOCK_CONFIG).await;

    assert_eq!(
        failures(&bench.events(), "tidy"),
        vec![Failure::paths_denied(
            vec![".codex/hooks.json".into()],
            vec![Protection::AgentConfig]
        )]
    );
}

/// What a committed link at a CLI's configuration resolves to is that
/// configuration: a node that writes it there fails without a grant, as a
/// write through the link would, and never lands.
#[cfg(unix)]
#[tokio::test]
async fn a_node_writing_what_a_cli_loads_through_a_link_fails_without_a_grant() {
    let bench = Bench::new();
    yunta_testkit::write(&bench.worktree.join("tools/hooks/run.sh"), "true\n");
    std::fs::create_dir_all(bench.worktree.join(".claude")).unwrap();
    std::os::unix::fs::symlink("../tools/hooks", bench.worktree.join(".claude/hooks")).unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "hooks"]);
    let workflow = "name: w\nnodes:\n  - { id: tidy, kind: bash, scope: [\"**\"], run: \"echo x > tools/hooks/run.sh && echo y > tools/notes.txt\" }\n";
    let fixture = "dialect: claude-code\nsessions: []\n";
    bench.run_with_config(workflow, fixture, MOCK_CONFIG).await;

    let failed = failures(&bench.events(), "tidy");
    assert_eq!(
        failed,
        vec![Failure::paths_denied(
            vec!["tools/hooks/run.sh".into()],
            vec![Protection::AgentConfig]
        )]
    );
    assert!(!failed[0].wants_scope(), "no grant widens it");
    let landed = git_output(&bench.worktree, &["show", "HEAD:tools/hooks/run.sh"]);
    assert_eq!(landed, "true");
}

/// The root of a tree a CLI loads is held as itself: a node that points a
/// committed link there elsewhere, or puts a link where none stood, fails
/// without a grant.
#[cfg(unix)]
#[tokio::test]
async fn a_node_pointing_a_cli_s_config_root_elsewhere_fails_without_a_grant() {
    let bench = Bench::new();
    yunta_testkit::write(&bench.worktree.join("config/claude/settings.json"), "{}\n");
    std::os::unix::fs::symlink("config/claude", bench.worktree.join(".claude")).unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "config"]);
    let workflow = "name: w\nnodes:\n  - { id: tidy, kind: bash, scope: [\"**\"], run: \"mkdir -p evil && echo {} > evil/settings.json && ln -sfn evil .claude && ln -s tools .codex\" }\n";
    let fixture = "dialect: claude-code\nsessions: []\n";
    bench.run_with_config(workflow, fixture, MOCK_CONFIG).await;

    assert_eq!(
        failures(&bench.events(), "tidy"),
        vec![Failure::paths_denied(
            vec![".claude".into(), ".codex".into()],
            vec![Protection::AgentConfig, Protection::AgentConfig]
        )]
    );
}

/// A link a node puts at the root of a tree a CLI loads is judged though git
/// ignores it: the close looks at what stands there, never under it.
#[cfg(unix)]
#[tokio::test]
async fn a_link_git_ignores_at_a_cli_s_config_root_still_fails_the_node() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - { id: tidy, kind: bash, scope: [\"**\"], run: \"echo /.codex >> .gitignore && mkdir -p tools && echo x > tools/config.toml && ln -s tools .codex\" }\n";
    let fixture = "dialect: codex\nsessions: []\n";
    bench.run_with_config(workflow, fixture, MOCK_CONFIG).await;

    assert_eq!(
        failures(&bench.events(), "tidy"),
        vec![Failure::paths_denied(
            vec![".codex".into()],
            vec![Protection::AgentConfig]
        )]
    );
}

/// The planner's plan and one task scoped to `scope` whose session writes
/// `writes`, under a `rules` expansion mode.
fn one_task(scope: &str, writes: &str, request: Option<&str>) -> (String, String) {
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-w", "w", scope, "test -f a.txt")
    );
    let mut fixture = plan_session(&tasks);
    let request = request
        .map(|paths| {
            request_step(serde_json::json!({
                "paths": [paths],
                "reason": "the workflow file is wrong",
            }))
        })
        .unwrap_or_default();
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-w\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: {writes:?}, content: \"x\" }}\n{request}    outcome: {{ type: completed, summary: did-w }}\n"
    ));
    (scope_expansion_workflow("rules", None), fixture)
}

#[tokio::test]
async fn a_task_writing_a_denied_path_is_rejected() {
    let bench = Bench::new();
    let (workflow, fixture) = one_task("**", ".github/ci.yml", None);
    bench
        .run_with_config(&workflow, &fixture, &denying_ci())
        .await;

    // The denied write is judged before it lands, so it is refused on the log.
    let events = bench.events();
    let refused: Vec<ToolTarget> = events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Session(SessionEvent::WriteRefused(p))) => Some(p.target.clone()),
            _ => None,
        })
        .collect();
    assert!(
        refused.contains(&ToolTarget::of_path(std::path::Path::new(".github/ci.yml"))),
        "the write the project denies is refused on the log: {refused:?}"
    );
    let landed = git_output(&bench.worktree, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(!landed.contains(".github/ci.yml"), "{landed}");
}

/// Under `rules`, a request for a denied path is refused by rule, without
/// asking anyone, and the refusal names the project's deny as its reason.
#[tokio::test]
async fn a_task_request_for_a_denied_path_is_refused_by_rule_without_asking() {
    let bench = Bench::new();
    let (workflow, fixture) = one_task("a.txt", "a.txt", Some(".github/ci.yml"));
    bench
        .run_with_config(&workflow, &fixture, &denying_ci())
        .await;

    let reasons: Vec<Denial> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Denied(p))) => p.why(),
            _ => None,
        })
        .collect();
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            Denial::ReachesProtected { reached }
                if reached.iter().any(|hit| hit.reason == Protection::ProjectDeny)
        )),
        "{reasons:?}"
    );
    assert!(!bench.events().iter().any(|event| matches!(
        event.payload(),
        Some(EventPayload::Scope(ScopeEvent::Granted(_)))
    )));
}

/// A request the project denies runs no proposed criterion. The request
/// reaches the judgement as a file a shell command left, so the attempt's end
/// decides it, and the criterion's command, which would leave a marker, never
/// runs.
#[tokio::test]
async fn a_request_the_project_denies_runs_no_proposed_criterion() {
    let bench = Bench::new();
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("marker");
    let request = format!(
        "paths: [.github/ci.yml]\nreason: the workflow file is wrong\nproposed_criterion:\n  cmd: \"touch {}\"\n",
        marker.display()
    );
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-w", "w", "a.txt", "test -f a.txt")
    );
    let fixture = format!(
        "{}  - match_prompt_contains: \"task-w\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: {SCOPE_EXPANSION_REQUEST_FILE:?}, content: {request:?}, via: command }}\n    outcome: {{ type: completed, summary: asked }}\n",
        plan_session(&tasks),
    );

    bench
        .run_with_config(
            &scope_expansion_workflow("rules", None),
            &fixture,
            &denying_ci(),
        )
        .await;

    assert!(
        !marker.exists(),
        "the criterion of a request the project denies never ran"
    );
    let reasons: Vec<Denial> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Denied(p))) => p.why(),
            _ => None,
        })
        .collect();
    assert!(
        reasons.iter().any(|reason| matches!(
            reason,
            Denial::ReachesProtected { reached }
                if reached.iter().any(|hit| hit.reason == Protection::ProjectDeny)
        )),
        "{reasons:?}"
    );
}

/// A node's session that asks for a denied path is never put to a
/// person: nothing else was asked, so its close goes on.
#[tokio::test]
async fn a_nodes_request_for_a_denied_path_is_never_put_to_a_person() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - { id: fix, kind: prompt, runner: executor, prompt: fix, scope: [\"src/**\"] }\n";
    let fixture = "dialect: claude-code\nsessions:\n  - steps:\n      - type: run_tool\n        tool: yunta_request_scope_expansion\n        arguments: { paths: [.github/ci.yml], reason: \"the workflow is wrong\" }\n    outcome: { type: completed, summary: asked }\n";
    let RunReport { terminal, .. } = bench
        .run_with_config(workflow, fixture, &denying_ci())
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(failures(&bench.events(), "fix").is_empty());
}

/// A node's request for a path the project denies is recorded as denied,
/// with the finding that says why, and the node's close goes on.
#[tokio::test]
async fn a_node_request_for_a_denied_path_is_denied_and_leaves_a_finding() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - { id: fix, kind: bash, scope: [\"src/**\"], run: \"printf 'paths: [\\\".github/ci.yml\\\"]\\nreason: ci\\n' > .yunta-scope-expansion-request.yaml\" }\n";
    bench
        .run_with_config(
            workflow,
            "dialect: claude-code\nsessions: []\n",
            &denying_ci(),
        )
        .await;
    let denied = bench.events().iter().any(|e| {
        matches!(
            e.payload(),
            Some(EventPayload::Scope(ScopeEvent::Denied(_)))
        )
    });
    assert!(denied, "{:?}", bench.events());
    assert_eq!(bench.findings_by("fix").len(), 1);
}

/// An ignored file under a protected tree is judged like any other changed
/// file: the node fails naming it, though git would not have shown it.
#[tokio::test]
async fn an_ignored_file_under_a_protected_tree_fails_the_node() {
    let bench = Bench::new();
    std::fs::write(
        bench.worktree.join(".gitignore"),
        ".claude/settings.local.json\n",
    )
    .unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "ignore"]);
    let workflow = "name: w\nnodes:\n  - { id: tidy, kind: bash, scope: [\"**\"], run: \"mkdir -p .claude && echo x > .claude/settings.local.json && echo y > notes.txt\" }\n";
    let config = format!("{MOCK_CONFIG}permissions:\n  paths:\n    deny: [\".claude/**\"]\n");
    bench
        .run_with_config(workflow, "dialect: claude-code\nsessions: []\n", &config)
        .await;

    let ignored = std::path::PathBuf::from(".claude/settings.local.json");
    assert!(failures(&bench.events(), "tidy").iter().any(|f| matches!(f,
        Failure::PathsDenied { denied_paths, .. } if *denied_paths == vec![ignored.clone()])));
}
