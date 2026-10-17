//! A red check and a node's exit, read as their typed facts: a baseline
//! regression names its command, exit code and what the suite printed, a
//! coverage shortfall names its measured figure and threshold, and a failed
//! node's exit names its code.

use super::*;

/// The regression a run paused on: the command that passed at baseline, the
/// exit code it fails with now, and what it printed last. Any other pause
/// fails the test that asked for one.
fn regression_of(terminal: RunTerminal) -> (String, i32, Vec<String>) {
    match terminal {
        RunTerminal::Paused {
            reason:
                PauseReason::NodeFailed {
                    failure:
                        Failure::Red {
                            red:
                                RedCheck::Regression {
                                    cmd,
                                    exit_code,
                                    tail,
                                },
                        },
                    ..
                },
        } => (cmd, exit_code, tail),
        other => panic!("expected a regression to pause the run, got {other:?}"),
    }
}

#[tokio::test]
async fn the_first_baseline_compare_fails_on_a_regression_made_before_it() {
    let bench = Bench::new();
    // `cat marker.txt` exits 0 while the file is there — what the run
    // measures on its first wake, and what `regress` then breaks.
    tokio::fs::write(bench.worktree.join("marker.txt"), "ok\n")
        .await
        .unwrap();

    let workflow = r#"
name: baseline-regression
nodes:
  - id: regress
    kind: bash
    run: "rm marker.txt"
  - id: no-regressions
    kind: check
    builtin: baseline_compare
    depends_on: [regress]
"#;

    let RunReport { terminal, state: _ } = bench
        .run_with_config(workflow, NO_SESSIONS, CONFIG_WITH_BASELINE)
        .await;
    // A run's only `baseline_compare` compares against the measurement its
    // first wake took, so it must catch the regression.
    let (cmd, exit_code, tail) = regression_of(terminal);
    assert_eq!(cmd, "cat marker.txt");
    assert_eq!(exit_code, 1);
    assert!(
        tail.iter()
            .any(|line| line.contains("marker.txt: No such file")),
        "expected a regression quoting what the suite printed, got: {tail:?}"
    );
}

#[tokio::test]
async fn coverage_gate_fails_when_measured_coverage_is_below_the_threshold() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("coverage.txt"), "lines: 40.0%\n").unwrap();

    let workflow = r#"
name: coverage-low
nodes:
  - id: coverage
    kind: check
    builtin: coverage_gate
"#;

    let RunReport { terminal, state: _ } = bench
        .run_with_config(workflow, NO_SESSIONS, CONFIG_WITH_COVERAGE)
        .await;
    match terminal {
        RunTerminal::Paused {
            reason:
                PauseReason::NodeFailed {
                    node,
                    failure:
                        Failure::Red {
                            red:
                                RedCheck::Coverage {
                                    measured,
                                    threshold,
                                    ..
                                },
                        },
                },
        } => {
            assert_eq!(node.as_str(), "coverage");
            assert_eq!((measured, threshold), (Some(40.0), 80.0));
        }
        other => panic!("expected the coverage gate to pause the run, got {other:?}"),
    }
}

#[tokio::test]
async fn a_node_that_failed_is_on_the_log_as_failed_before_anything_else_runs() {
    // What a retry or a corrective node reads next has to know about the
    // failure it follows, even when nothing finished after it.
    let bench = Bench::new();
    let workflow = r#"
name: one-failure
nodes:
  - id: broken
    kind: bash
    run: "exit 3"
"#;
    let RunReport { terminal, state } = bench.run(workflow, NO_SESSIONS).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let Some(yunta_engine::NodeState::Failed {
        failure: Failure::Exited { exited },
        ..
    }) = state.nodes.state("broken")
    else {
        panic!("{:?}", state.nodes.state("broken"));
    };
    assert_eq!(exited.code, 3);
}
