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
    let fixture = plan_session(&tasks) + TASK_SESSIONS;

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

/// A `parallel` group of one prompt node, the run's only work.
const GROUPED: &str = r#"
name: grouped
nodes:
  - id: group
    kind: parallel
    nodes:
      - { id: inner, kind: prompt, runner: executor, prompt: "Do the work." }
"#;

/// A run in a mode shares the cap among the nodes the mode reaches, the
/// children of a group it includes among them, as a run without modes does.
#[tokio::test]
async fn a_mode_shares_the_cap_with_the_children_of_a_group_it_includes() {
    let session =
        "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: done }\n";
    let moded = format!("{GROUPED}modes:\n  only: {{ include: [group] }}\n");
    let mut shares = Vec::new();
    for (workflow, bench) in [
        (GROUPED.to_string(), Bench::new()),
        (moded, Bench::new().in_mode("only")),
    ] {
        bench.run_with_config(&workflow, session, CAPPED).await;
        let seen: Vec<Option<u64>> = bench
            .mock()
            .requests_seen()
            .into_iter()
            .map(|request| request.budget.max_tokens)
            .collect();
        shares.push(seen);
    }
    // The group and its child are both owed the cap: half of it each.
    assert_eq!(shares, [vec![Some(500)], vec![Some(500)]]);
}
