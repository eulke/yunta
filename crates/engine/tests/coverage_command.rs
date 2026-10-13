use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []";

/// A coverage command that exits non-zero fails the gate, whatever figure
/// it printed: the exit code is read before the percentage.
#[tokio::test]
async fn a_coverage_command_that_exits_non_zero_fails_the_gate() {
    let config = "runners:\n  planner: [{ adapter: mock, model: mock-model }]\n  executor: [{ adapter: mock, model: mock-model }]\ncoverage:\n  cmd: \"sh -c 'echo lines: 92.5%; exit 3'\"\n  threshold: 80.0\n";
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(
            "name: coverage-exit\nnodes:\n  - { id: coverage, kind: check, builtin: coverage_gate }\n",
            NO_SESSIONS,
            config,
        )
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
}
