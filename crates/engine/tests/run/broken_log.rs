//! A run whose log breaks before its close: the run is broken, not finished.

use super::*;

/// A log whose replay breaks before its `run_finished` is broken, not closed:
/// the replay never reaches the close the log carries.
#[tokio::test]
async fn a_log_that_breaks_before_its_close_is_broken_not_finished() {
    let bench = Bench::new();
    bench
        .create(
            "name: broken\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n",
            "dialect: claude-code\nsessions: []\n",
            MOCK_CONFIG,
        )
        .await;
    let failed = NodeFailedPayload::new(
        Failure::Message {
            outcome: "no start".into(),
        },
        TokenUsage::default(),
    );
    let close = RunFinishedPayload {
        terminal_state: TerminalState::Done,
        metrics: RunMetrics {
            cptv: None,
            tokens: TokenUsage::default(),
        },
        closed_by: None,
    };
    let run_id = bench.run_id.clone();
    let node_failed = EventDraft::new(
        run_id.clone(),
        Some("only".into()),
        EventPayload::Node(NodeEvent::Failed(failed)),
    )
    .expect("a node's failure names its node");
    let run_closed = EventDraft::new(run_id, None, EventPayload::Run(RunEvent::Finished(close)))
        .expect("a close names no subject");
    bench.storage.append(&node_failed, &FixedClock).unwrap();
    bench.storage.append(&run_closed, &FixedClock).unwrap();
    assert!(matches!(
        bench.try_wake().await,
        Err(RunError::Broken { .. })
    ));
}
