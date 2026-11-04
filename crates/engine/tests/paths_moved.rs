//! A move takes a file off the path it left as a delete does, so what no run
//! may change no run moves away: the close judges both paths of a move, with no
//! help from git, which on its own lists only where the file arrived. What a
//! project denies, what an agent's CLI loads and the instruction files all stay
//! where they were.

use yunta_core::events::{EventPayload, Failure, NodeEvent, StoredEvent};
use yunta_core::protected::Protection;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, git_output, write, Bench, MOCK_CONFIG};

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

fn denying_ci() -> String {
    format!("{MOCK_CONFIG}permissions:\n  paths:\n    deny: [\".github/**\"]\n")
}

/// A run of one `kind: bash` node, whose scope admits every path, that moves
/// `from` to `moved.txt`, on a repository whose last commit holds `from`, with
/// what it left at `HEAD`.
async fn moving(from: &str, config: &str) -> (Bench, RunTerminal, String) {
    let bench = Bench::new();
    write(&bench.worktree.join(from), "on: push\n");
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "seed"]);
    let workflow = format!(
        "name: w\nnodes:\n  - {{ id: tidy, kind: bash, scope: [\"**\"], run: \"mv {from} moved.txt\" }}\n"
    );
    let RunReport { terminal, .. } = bench
        .run_with_config(&workflow, "dialect: claude-code\nsessions: []\n", config)
        .await;
    let landed = git_output(&bench.worktree, &["ls-tree", "-r", "--name-only", "HEAD"]);
    (bench, terminal, landed)
}

/// Moving what the project denies fails as deleting it would: with no grant to
/// offer, and with the file where it was.
#[tokio::test]
async fn a_node_moving_a_denied_path_fails_without_a_grant_and_never_lands() {
    let (bench, terminal, landed) = moving(".github/ci.yml", &denying_ci()).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let failed = failures(&bench.events(), "tidy");
    assert_eq!(
        failed,
        vec![Failure::paths_denied(
            vec![".github/ci.yml".into()],
            vec![Protection::ProjectDeny]
        )]
    );
    assert!(!failed[0].wants_scope(), "no grant widens a deny");
    assert!(landed.contains(".github/ci.yml"), "{landed}");
    assert!(!landed.contains("moved.txt"), "{landed}");
}

/// What an agent's CLI loads from one file sits under no tree a sandbox can
/// deny, so the close is what keeps it where the CLI looks for it.
/// Reached by: `crates/engine/src/scope.rs`, `crates/engine/src/run/node_scope.rs`
#[tokio::test]
async fn a_node_moving_an_agent_s_own_config_fails_and_never_lands() {
    let (bench, terminal, landed) = moving(".mcp.json", MOCK_CONFIG).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        failures(&bench.events(), "tidy"),
        vec![Failure::paths_denied(
            vec![".mcp.json".into()],
            vec![Protection::AgentConfig]
        )]
    );
    assert!(landed.contains(".mcp.json"), "{landed}");
}

#[tokio::test]
async fn a_node_moving_an_instruction_file_fails_and_never_lands() {
    let (bench, terminal, landed) = moving("AGENTS.md", MOCK_CONFIG).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        failures(&bench.events(), "tidy"),
        vec![Failure::paths_denied(
            vec!["AGENTS.md".into()],
            vec![Protection::AgentInstructions]
        )]
    );
    assert!(landed.contains("AGENTS.md"), "{landed}");
    assert!(!landed.contains("moved.txt"), "{landed}");
}
