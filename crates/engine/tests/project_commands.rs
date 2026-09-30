//! A node runs a command the project declares by the name the workflow
//! gives it: as the project wrote it, under the run's permissions, and
//! failing plainly when the run's frozen config does not declare it.

use yunta_core::events::{EventPayload, Failure, NodeEvent, StoredEvent};
use yunta_core::{CommandName, ConfigKey};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, MOCK_CONFIG};

fn config(extra: &str) -> String {
    format!("{MOCK_CONFIG}{extra}")
}

fn node_failures(events: &[StoredEvent]) -> Vec<(Failure, bool)> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => {
                Some((p.failure.clone(), p.retryable))
            }
            _ => None,
        })
        .collect()
}

/// A project's command is the project's text, not the workflow's: the
/// run renders no template into it.
#[tokio::test]
async fn a_project_command_runs_as_written() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(
            "name: w\nnodes:\n  - { id: make, kind: bash, run: { command: stamp } }\n",
            "sessions: []\n",
            &config("commands:\n  stamp: \"printf '%s' '{{run.branch}}' > stamp.txt\"\n"),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let written = tokio::fs::read_to_string(bench.worktree.join("stamp.txt"))
        .await
        .unwrap();
    assert_eq!(written, "{{run.branch}}");
}

#[tokio::test]
async fn a_hook_runs_a_project_command() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - id: work\n    kind: bash\n    run: \"true\"\n    hooks: { after: [{ run: { command: note } }] }\n";
    let RunReport { terminal, .. } = bench
        .run_with_config(
            workflow,
            "sessions: []\n",
            &config("commands:\n  note: \"echo noted > noted.txt\"\n"),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let ran: Vec<String> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::HookExecuted(p))) => Some(p.command.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(ran, vec!["echo noted > noted.txt".to_string()]);
    assert!(bench.worktree.join("noted.txt").exists());
}

/// No attempt of the run can change its frozen config, so nothing
/// retries the node.
#[tokio::test]
async fn a_command_the_frozen_config_lacks_fails_unset_and_not_retryably() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(
            "name: w\nnodes:\n  - { id: lint, kind: bash, run: { command: lint } }\n",
            "sessions: []\n",
            &config(""),
        )
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        node_failures(&bench.events()),
        vec![(
            Failure::unset(ConfigKey::Command {
                command: CommandName::from("lint")
            }),
            false
        )]
    );
}

#[tokio::test]
async fn permissions_commands_govern_a_project_command() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(
            "name: w\nnodes:\n  - { id: clean, kind: bash, run: { command: wipe } }\n",
            "sessions: []\n",
            &config(
                "commands:\n  wipe: \"rm -rf build\"\npermissions:\n  commands:\n    deny: [\"rm -rf*\"]\n",
            ),
        )
        .await;

    match terminal {
        RunTerminal::Paused { reason } => assert_eq!(
            reason,
            "node `clean` failed: command `rm -rf build` matches denied pattern `rm -rf*` (permissions.commands.deny)"
        ),
        other => panic!("expected the run to pause, got {other:?}"),
    }
}
