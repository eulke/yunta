//! A `join: any` group whose children all fail names every one of them.

use yunta_core::events::{Failure, PauseReason};
use yunta_core::JoinPolicy;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

/// A `join: any` group of two children that both exit with a failure.
const RACE_OF_FAILURES: &str = r#"
name: race
nodes:
  - id: race
    kind: parallel
    join: any
    nodes:
      - id: first
        kind: bash
        run: "exit 1"
      - id: second
        kind: bash
        run: "exit 1"
"#;

#[tokio::test]
async fn a_parallel_group_with_join_any_fails_naming_every_child_when_all_fail() {
    let bench = Bench::new();

    let RunReport { terminal, .. } = bench
        .run(RACE_OF_FAILURES, "dialect: claude-code\nsessions: []")
        .await;
    let failure = match terminal {
        RunTerminal::Paused {
            reason: PauseReason::NodeFailed { node, failure },
        } => {
            assert_eq!(node.as_str(), "race");
            failure
        }
        other => panic!("expected Paused, got {other:?}"),
    };

    let Failure::ChildrenFailed {
        join,
        children_failed,
    } = &failure
    else {
        panic!("expected ChildrenFailed, got {failure:?}");
    };
    assert_eq!(join, &JoinPolicy::Any);
    // Both children run at once, so the order they fail in is not fixed.
    let mut named: Vec<&str> = children_failed.iter().map(|id| id.as_str()).collect();
    named.sort_unstable();
    assert_eq!(named, ["first", "second"], "{failure:?}");
    assert_eq!(
        yunta_render::failure::sentence(&failure),
        "join: any — no child succeeded (2 failed)"
    );
}
