//! A receipt derived from a real run's own log, and refused for one that never ended.

use super::*;

/// Exercises every receipt section in one run: a task with two
/// criteria (`plan`/`implement`), two baseline comparisons
/// (`compare-early`/`compare`), a reroute (`lint` fails once,
/// `fix-lint` corrects it), and a fan-out review (`runners: [reviewer,
/// reviewer-alt]`) whose effects stay inside its own declared `scope`.
const WORKFLOW: &str = r#"
name: receipt-fixture
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "implement your task"
  - id: compare-early
    kind: check
    builtin: baseline_compare
    depends_on: [implement]
  - id: compare
    kind: check
    builtin: baseline_compare
    depends_on: [compare-early]
  - id: lint
    kind: bash
    depends_on: [compare]
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 2 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "fix the lint failure"
  - id: review
    kind: prompt
    depends_on: [lint]
    runners: [reviewer, reviewer-alt]
    scope: ["notes/**", "hello.txt", "fixed.txt"]
    prompt: "review the change"
"#;

/// The tasks document reaches the run through the run tools, so the planning
/// session names no path at all; every other session's `path` is
/// relative to the worktree, which is a session's own cwd.
const FIXTURE: &str = r#"
dialect: claude-code
sessions:
  - match_prompt_contains: "Write the tasks document"
    steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            tasks:
              - id: T001
                title: "Say hello"
                scope: ["hello.txt"]
                criteria:
                  - cmd: "test -f hello.txt"
    outcome: { type: completed, summary: "planned" }
  - match_prompt_contains: "implement your task"
    effects:
      - { path: hello.txt, content: "hi" }
    outcome: { type: completed, summary: "done" }
  - match_prompt_contains: "fix the lint failure"
    effects:
      - { path: fixed.txt, content: "fixed" }
    outcome: { type: completed, summary: "fixed it" }
  - match_prompt_contains: "review the change"
    effects:
      - { path: notes/reviewer.md, content: "looks good" }
    outcome: { type: completed, summary: "reviewed" }
  - match_prompt_contains: "review the change"
    effects:
      - { path: notes/reviewer-alt.md, content: "also good" }
    outcome: { type: completed, summary: "reviewed" }
"#;

/// The commands a receipt's criteria summary counts, in its order.
fn commands(criteria: &CriteriaSummary) -> Vec<&str> {
    criteria
        .entries
        .iter()
        .map(|entry| entry.cmd.as_str())
        .collect()
}

#[tokio::test]
async fn build_receipt_derives_every_section_from_a_real_runs_own_log() {
    let bench = Bench::new();
    let RunReport { .. } = bench.run_with_config(WORKFLOW, FIXTURE, CONFIG).await;
    let manifest = bench.manifest();
    let events = bench.events();

    let chain = EventChainStatus::Intact {
        events: events.len(),
    };
    let receipt = receipt::closed(&bench.run_id, &manifest, &events, chain).unwrap();

    assert_eq!(receipt.terminal_state, Some(TerminalState::Done));

    // T001's one criterion, and the suite the run holds every task to.
    assert_eq!(commands(&receipt.criteria), ["test -f hello.txt", "true"]);
    assert_eq!(receipt.criteria.green, 2);

    let baseline = receipt.baseline.clone().expect("baseline_compare was used");
    assert_eq!(baseline.suite, "true");
    assert_eq!(
        baseline.compared, 2,
        "every `baseline_compare` compares against the capture the run's birth took"
    );
    assert_eq!(baseline.regressions, 0);

    assert!(
        receipt.scope.violations.is_empty(),
        "every session wrote inside its declared scope: {:?}",
        receipt.scope.violations
    );
    assert!(receipt.scope.files_touched > 0);

    let groups = receipt.fan_out_groups();
    assert_eq!(groups.len(), 1);
    let (base, members) = &groups[0];
    assert_eq!(base, "review");
    assert_eq!(members.len(), 2);

    assert_eq!(receipt.cost.reroutes, 1, "lint failed once, then rerouted");
    assert!(receipt.cost.cptv.is_some(), "one task reached done");

    // The formatters accept whatever receipt::closed derived without
    // panicking or needing extra data — same contract a real `pr` node
    // relies on.
    let markdown = render_receipt_markdown(&receipt);
    assert!(
        markdown.contains("by 2 independent runners"),
        "got: {markdown}"
    );
    serde_json::to_string_pretty(&receipt).unwrap();
}

#[tokio::test]
async fn build_receipt_refuses_a_run_that_never_reached_run_finished() {
    let bench = Bench::new();
    // `lint` fails with no `on_failure` at all — the run pauses, never
    // finishes, so there is no `run_finished` metrics to certify.
    let workflow = r#"
name: never-finishes
nodes:
  - id: broken
    kind: bash
    run: "false"
"#;
    let RunReport { .. } = bench
        .run_with_config(workflow, "dialect: claude-code\nsessions: []\n", CONFIG)
        .await;

    let manifest = bench.manifest();
    let events = bench.events();
    let err = receipt::closed(
        &bench.run_id,
        &manifest,
        &events,
        EventChainStatus::Intact {
            events: events.len(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, receipt::ReceiptError::NotFinished(_)));
    assert_eq!(
        err.to_string(),
        format!(
            "run `{}` hasn't reached a terminal state yet: a receipt is only generated \
             once a run finishes",
            bench.run_id
        ),
        "the engine names the run's state; which command shows where it stands is the \
         caller's vocabulary"
    );
}
