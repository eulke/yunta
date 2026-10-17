//! What a red criterion printed is the run's to keep, whoever asked —
//! a session's own check, a document's probes, the loop's checks — and
//! whoever reads that answer later reads all of it, not only the last
//! lines the log quotes.

use yunta_core::events::{Decider, EventPayload, ScopeEvent, TaskEvent, TaskStatus};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, Bench};

mod common;
use common::*;

/// Red until `b.rs` exists: it points there on its first line, then
/// prints far more than the last lines a log quotes.
const CHECK: &str = "test -f b.rs && exit 0\necho '  --> b.rs:3:9'\nseq 200\nexit 1\n";

/// A plan, a person approving it, and a loop that grants on evidence.
const WORKFLOW: &str = "name: kept\nnodes:\n  - id: plan\n    kind: prompt\n    runner: planner\n    prompt: \"Hand over the tasks document.\"\n    artifacts:\n      produces: [tasks]\n  - id: approve\n    kind: gate\n    depends_on: [plan]\n    assignee: lead\n    message: \"Approve the plan?\"\n    options: [approve]\n  - id: implement\n    kind: loop\n    runner: executor\n    depends_on: [approve]\n    until: all_tasks_complete\n    prompt: \"Implement your task.\"\n    scope_expansion:\n      mode: ask\n";

/// A bench whose repository holds the check `task-h` is judged by, run
/// with an environment of its own so a wake takes in what was answered.
fn bench() -> Bench {
    let bench = Bench::new().with_subprocess_vars(Vec::new());
    std::fs::write(bench.worktree.join("check.sh"), CHECK).unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "check"]);
    bench
}

/// The planner hands `task-h` over; its session does `first`, and the
/// session picked back up once `b.rs` is granted writes it.
fn fixture(first: &str) -> String {
    let mut fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("task-h", "h", "a.rs", "sh check.sh")
    ));
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-h\"\n{first}    outcome: {{ type: completed, summary: did-h }}\n\
         \x20 - match_prompt_contains: \"was granted\"\n    effects:\n      - {{ path: b.rs, content: \"b\" }}\n    outcome: {{ type: completed, summary: did-b }}\n"
    ));
    fixture
}

#[tokio::test]
async fn a_sessions_check_keeps_all_a_red_criterion_printed() {
    let bench = bench();
    let checks = "    effects:\n      - { path: a.rs, content: \"a\" }\n    steps:\n      - { type: run_tool, tool: yunta_check_task }\n";

    bench.run(WORKFLOW, &fixture(checks)).await;
    answer_parked(&bench, "approve").await.unwrap();
    bench.wake().await;

    let kept = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Tasks(TaskEvent::CheckAnswered(check))) => check
                .results
                .first()
                .and_then(|result| result.output.clone()),
            _ => None,
        });
    let printed = bench.object(&kept.expect("the red answer names what it printed"));
    assert!(String::from_utf8_lossy(&printed.unwrap()).starts_with("  --> b.rs:3:9"));
}

#[tokio::test]
async fn evidence_reads_a_location_beyond_the_last_lines_of_an_answer_from_the_log() {
    let bench = bench();
    let request = request_step(serde_json::json!({
        "paths": ["b.rs"],
        "reason": "the caller breaks",
        "evidence": "sh check.sh",
    }));

    bench.run(WORKFLOW, &fixture(&request)).await;
    answer_parked(&bench, "approve").await.unwrap();
    let RunReport { terminal, state } = bench.wake().await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-h"), Some(TaskStatus::Done));
    let granted = bench
        .events()
        .into_iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Granted(granted))) => {
                Some(granted.decided_by.clone())
            }
            _ => None,
        });
    let evidence = Decider::Evidence {
        criterion: "sh check.sh".into(),
    };
    assert_eq!(granted, Some(evidence));
}
