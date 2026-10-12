//! A project command `baseline.held` names — its lint, for one — is
//! measured beside the suite, and holds every task as the suite does when
//! it passed before the run changed anything.

use yunta_core::events::{
    BaselineCapturedPayload, BaselineHeldPayload, BaselineOrigin, BaselineResults, EventPayload,
    NodeEvent, Phase, RunEvent,
};
use yunta_core::{Criterion, Task};
use yunta_engine::{judged_task, RunReport, RunTerminal};
use yunta_testkit::{Bench, MOCK_CONFIG};

/// The suite, here a check that `broken.txt` is absent.
const SUITE: &str = "test ! -f broken.txt";

/// What a run whose log holds `measurements` holds its tasks to.
fn held(measurements: Vec<RunEvent>) -> Vec<String> {
    let log = measurements
        .into_iter()
        .fold(
            yunta_testkit_core::Log::for_run("run-held"),
            |log, event| log.event(EventPayload::Run(event)),
        )
        .build();
    yunta_engine::held_of(&yunta_engine::derive(&log).run)
}

/// The suite, measured green.
fn suite() -> RunEvent {
    RunEvent::BaselineCaptured(BaselineCapturedPayload {
        command: SUITE.to_string(),
        results: BaselineResults {
            exit_code: 0,
            summary: String::new(),
        },
        hash: yunta_core::sha256_hex(b""),
        origin: BaselineOrigin::Measured,
        tree: None,
        duration_ms: None,
    })
}

/// The project's `lint`, measured beside the suite with `exit_code`.
fn lint(exit_code: i32) -> RunEvent {
    RunEvent::BaselineHeld(BaselineHeldPayload {
        name: "lint".into(),
        command: "cargo clippy".to_string(),
        results: BaselineResults {
            exit_code,
            summary: String::new(),
        },
        origin: BaselineOrigin::Measured,
        tree: None,
        duration_ms: None,
    })
}

fn task() -> Task {
    Task {
        id: "T001".into(),
        title: "Make it".to_string(),
        scope: vec!["made.txt".into()],
        criteria: vec![Criterion {
            cmd: "test -f made.txt".to_string(),
            r#type: None,
            proves: None,
        }],
        depends_on: Vec::new(),
        notes: None,
        description: None,
        changes: Vec::new(),
        outcome: None,
        uses: Vec::new(),
        invariants: Vec::new(),
    }
}

/// Each criterion a task is judged by, as `(cmd, is_guard)`.
fn commands(task: &Task) -> Vec<(String, bool)> {
    task.criteria
        .iter()
        .map(|criterion| (criterion.cmd.clone(), criterion.is_guard()))
        .collect()
}

/// A command `baseline.held` names holds every task beside the suite when
/// it passed before the run changed anything — first, so a red one
/// answers a check before the suite runs — and holds nothing when it was
/// already red.
#[test]
fn a_held_command_green_at_the_base_holds_every_task_and_a_red_one_holds_none() {
    let task = task();
    let green = held(vec![suite(), lint(0)]);
    assert_eq!(
        commands(&judged_task(&task, &green)),
        vec![
            ("test -f made.txt".to_string(), false),
            ("cargo clippy".to_string(), true),
            (SUITE.to_string(), true)
        ]
    );
    let red = held(vec![suite(), lint(1)]);
    assert_eq!(
        commands(&judged_task(&task, &red)),
        vec![
            ("test -f made.txt".to_string(), false),
            (SUITE.to_string(), true)
        ]
    );
}

// --- a run's loop -----------------------------------------------------------

/// A plan that writes a one-task document, and the loop that works it.
const WORKFLOW: &str = r#"
name: held
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: Make it\n    scope: [made.txt]\n    criteria:\n      - cmd: test -f made.txt\n' > {{node.artifacts}}/tasks.yaml"
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

/// A session that does the task's work.
const SESSION: &str = "\
dialect: claude-code
sessions:
  - effects:
      - { path: made.txt, content: made }
    outcome: { type: completed, summary: made }
";

/// The project's `lint`, here a check that `unformatted.txt` is absent.
const LINT: &str = "test ! -f unformatted.txt";

fn config_holding_lint() -> String {
    format!(
        "{MOCK_CONFIG}commands:\n  lint: \"{LINT}\"\nbaseline:\n  suite: \"{SUITE}\"\n  held: [lint]\n"
    )
}

/// What the run measured of each held command, by name and exit code.
fn held_measured(bench: &Bench) -> Vec<(String, i32)> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Run(RunEvent::BaselineHeld(p))) => {
                Some((p.name.to_string(), p.results.exit_code))
            }
            _ => None,
        })
        .collect()
}

/// Every command T001's close ran, in order.
fn post_checked(bench: &Bench) -> Vec<String> {
    bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(p))) if p.phase == Phase::Post => {
                Some(p.results.clone())
            }
            _ => None,
        })
        .unwrap_or_default()
        .into_iter()
        .map(|result| result.cmd)
        .collect()
}

#[tokio::test]
async fn a_command_held_beside_the_suite_and_green_at_the_base_holds_every_task() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(WORKFLOW, SESSION, &config_holding_lint())
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(held_measured(&bench), vec![("lint".to_string(), 0)]);
    assert_eq!(
        post_checked(&bench),
        vec![
            "test -f made.txt".to_string(),
            LINT.to_string(),
            SUITE.to_string()
        ],
        "the held command runs first, then the suite"
    );
}

#[tokio::test]
async fn a_command_held_beside_the_suite_and_red_at_the_base_holds_no_task() {
    let bench = Bench::new();
    tokio::fs::write(bench.worktree.join("unformatted.txt"), "x")
        .await
        .unwrap();
    yunta_testkit::git(&bench.worktree, &["add", "-A"]);
    yunta_testkit::git(&bench.worktree, &["commit", "-q", "-m", "red lint"]);
    let RunReport { terminal, .. } = bench
        .run_with_config(WORKFLOW, SESSION, &config_holding_lint())
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(held_measured(&bench), vec![("lint".to_string(), 1)]);
    assert!(
        !post_checked(&bench).contains(&LINT.to_string()),
        "no task could keep green what never was"
    );
}
