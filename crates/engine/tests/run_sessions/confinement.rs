//! What a session may write and reach, as the engine resolves it for each node
//! and as the log records it, and the refusals the engine makes before a session
//! opens.

use super::*;
use yunta_core::events::ConfinementRecord;
use yunta_core::port::PermissionProfile;
use yunta_core::{Commands, Worktree};

/// The project's default opens the network, but a read-only node's session
/// is handed a closed one: a session that writes nothing of the checkout is
/// confined with its network closed.
#[tokio::test]
async fn a_read_only_node_is_offered_no_network_whatever_the_project_says() {
    let workflow = "name: w\nnodes:\n  - id: r\n    kind: prompt\n    runner: executor\n    permissions: read-only\n    prompt: \"Read.\"\n";
    let bench = Bench::new();
    let config = format!(
        "{}permissions:\n  network:\n    default: true\n",
        yunta_testkit::MOCK_CONFIG
    );
    bench
        .run_with_config(
            workflow,
            "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: r }\n",
            &config,
        )
        .await;
    let networks: Vec<bool> = bench
        .mock()
        .requests_seen()
        .iter()
        .map(|req| req.confinement.network)
        .collect();
    assert_eq!(networks, [false]);
}

/// A shared directory inside the checkout refuses the session before it opens:
/// the node fails on the refusal, which names the directory, and no request
/// reaches the agent with a write grant the checkout's scope cannot judge.
#[tokio::test]
async fn a_shared_directory_inside_the_checkout_is_refused_before_any_session_opens() {
    let bench = Bench::new();
    let inside = bench.worktree.join("cache");
    let config = format!(
        "{}shared_dirs:\n  SHARED_CACHE: {}\n",
        yunta_testkit::MOCK_CONFIG,
        inside.display()
    );
    let workflow =
        "name: w\nnodes:\n  - id: r\n    kind: prompt\n    runner: executor\n    prompt: \"Go.\"\n";
    let RunReport { terminal, state } = bench
        .run_with_config(
            workflow,
            "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: r }\n",
            &config,
        )
        .await;

    let refusal = format!(
        "the shared directory `{}` lies inside the checkout; a shared directory must lie outside it",
        inside.display()
    );
    match &terminal {
        RunTerminal::Paused { reason } => {
            assert!(node_failed(reason, "r", &refusal), "{reason:?}");
        }
        other => panic!("a shared directory inside the checkout fails its node, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.state("r"),
        Some(NodeState::Failed { .. })
    ));
    assert!(
        bench.mock().requests_seen().is_empty(),
        "a session opened with a shared directory inside its checkout"
    );
}

/// The confinement the log records for the session `node` opened.
fn opened_confinement(bench: &Bench, node: &str) -> ConfinementRecord {
    bench
        .events()
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Session(SessionEvent::Opened(p)))
                if e.node_id.as_ref().is_some_and(|id| id.as_str() == node) =>
            {
                p.confinement.clone()
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("`{node}`'s session records what it may write and reach"))
}

/// What a session records it may write and reach: a read-only node's session
/// has no checkout to write and no network, and a node with `permissions: full`
/// runs unconfined over the whole checkout with its network open.
#[tokio::test]
async fn the_opened_session_records_what_it_may_write_and_reach() {
    let workflow = r#"
name: w
nodes:
  - id: reader
    kind: prompt
    runner: executor
    permissions: read-only
    prompt: "Survey the tree."
  - id: rebuild
    kind: prompt
    runner: executor
    permissions: full
    prompt: "Rebuild the tree."
"#;
    let fixture = r#"
dialect: claude-code
sessions:
  - match_prompt_contains: "Survey the tree."
    outcome: { type: completed, summary: surveyed }
  - match_prompt_contains: "Rebuild the tree."
    outcome: { type: completed, summary: rebuilt }
"#;
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let reader = opened_confinement(&bench, "reader");
    assert_eq!(reader.profile, PermissionProfile::ReadOnly);
    assert_eq!(reader.commands, Commands::Confined);
    assert_eq!(reader.worktree, Worktree::Nothing);
    assert!(!reader.network);

    let rebuild = opened_confinement(&bench, "rebuild");
    assert_eq!(rebuild.profile, PermissionProfile::Full);
    assert_eq!(rebuild.commands, Commands::Unconfined);
    assert_eq!(rebuild.worktree, Worktree::All);
    assert!(rebuild.network);
}
