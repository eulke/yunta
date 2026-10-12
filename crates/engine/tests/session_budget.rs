//! A session's share of the run's token cap is read from what the run has
//! spent so far: settled work, and the usage an open attempt reported that
//! nothing has settled.

mod common;

use common::{plan_session, task_yaml, LOOP_CAP_WORKFLOW};
use yunta_testkit::Bench;

const CAPPED: &str = r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
  executor:
    - { adapter: mock, model: mock-model }
limits:
  max_tokens_per_run: 1000
"#;

/// The two tasks' sessions, in dispatch order: the first reports 400 tokens.
const TASK_SESSIONS: &str = r#"  - steps:
      - { type: usage, input_tokens: 300, output_tokens: 100 }
    effects:
      - { path: a.txt, content: "a" }
    outcome: { type: completed, summary: "did T001" }
  - effects:
      - { path: b.txt, content: "b" }
    outcome: { type: completed, summary: "did T002" }
"#;

/// Two tasks of one loop run one after the other. The first task's session
/// reports 400 tokens, which stay in flight while the loop node is open, so
/// the second task's session is handed what is left of the cap.
#[tokio::test]
async fn a_session_s_share_of_the_cap_counts_what_an_open_attempt_reported() {
    let tasks = format!(
        "tasks:\n{}{}    depends_on: [T001]\n",
        task_yaml("T001", "a", "a.txt", "test -f a.txt"),
        task_yaml("T002", "b", "b.txt", "test -f b.txt"),
    );
    // The adapter reports usage, so the run hands its sessions a cap.
    let planned = plan_session(&tasks).replacen(
        "capabilities: { run_tools: true }",
        "capabilities: { run_tools: true, usage_reporting: true }",
        1,
    );
    let fixture = planned + TASK_SESSIONS;

    let bench = Bench::new();
    bench
        .run_with_config(LOOP_CAP_WORKFLOW, &fixture, CAPPED)
        .await;

    let shares: Vec<Option<u64>> = bench
        .mock()
        .requests_seen()
        .into_iter()
        .map(|request| request.budget.max_tokens)
        .collect();
    // The planner shares the cap with the loop; the first task has it
    // whole; the second has what the first left.
    assert_eq!(shares, [Some(500), Some(1000), Some(600)]);
}
