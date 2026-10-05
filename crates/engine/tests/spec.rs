//! A spec: the tests a plan's tasks are held to, written before any task
//! is built and by someone other than whoever builds it.
//!
//! Handed over, it is held to the plan the run holds and proven where
//! its tests run: every task it names is the plan's, and with every one
//! of its files written into the run's tree, each test runs and fails.
//! In the loop, a task's tests are laid over the tree its work starts
//! from, close it with its own criteria, and are the one thing its work
//! may not change.

mod common;

use common::spec::*;
use common::*;
use yunta_core::events::{ArtifactEvent, EventPayload, NodeEvent, TaskStatus};
use yunta_core::ArtifactKind;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, MOCK_CONFIG};

#[tokio::test]
async fn a_spec_whose_tests_fail_before_the_work_is_accepted_with_its_files_in_the_tree() {
    let spec = spec_of("greet", GREETS, "sh tests/greet.sh");
    let fixture = plan_session(PLAN) + &specifying(&[(&spec, true)]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(WORKFLOW, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(refused(&bench).is_empty(), "{:?}", refused(&bench));
}

#[tokio::test]
async fn a_spec_whose_test_already_passes_or_names_no_task_of_the_plan_is_refused() {
    let passes = spec_of("greet", "exit 0\n", "sh tests/greet.sh");
    let strays = spec_of("farewell", GREETS, "sh tests/greet.sh");
    let holds = spec_of("greet", GREETS, "sh tests/greet.sh");
    let fixture =
        plan_session(PLAN) + &specifying(&[(&passes, false), (&strays, false), (&holds, true)]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(WORKFLOW, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        refused(&bench),
        [
            vec!["criterion-already-passes".to_string()],
            vec!["unknown-spec-task".to_string()],
        ]
    );
}

#[tokio::test]
async fn a_task_closes_on_the_tests_its_spec_gives_it_and_they_land_with_its_work() {
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run(
            &specified_loop(),
            &building("      - { path: greeting.txt, content: Hello }\n"),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
    let pre: Vec<String> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(p)))
                if p.phase == yunta_core::events::Phase::Pre =>
            {
                Some(p.results.iter().map(|r| r.cmd.clone()).collect::<Vec<_>>())
            }
            _ => None,
        })
        .flatten()
        .collect();
    assert!(
        pre.contains(&"sh tests/greet.sh".to_string()),
        "the spec's test is among the task's criteria: {pre:?}"
    );
    let landed = tokio::fs::read_to_string(bench.worktree.join("tests/greet.sh"))
        .await
        .expect("the test file is in the run's tree");
    assert_eq!(landed, GREETS);
}

/// Two tasks the loop works at once, each held to a test of its own.
const TWO_TASKS: &str = "\
tasks:
  - id: greet
    title: \"Write the greeting\"
    scope: [\"greeting.txt\"]
    criteria:
      - cmd: \"test -f greeting.txt\"
  - id: wave
    title: \"Write the wave\"
    scope: [\"wave.txt\"]
    criteria:
      - cmd: \"test -f wave.txt\"
";

/// A spec giving `greet` and `wave` a test each.
const TWO_SPECS: &str = "{ specs: [\
    { task: greet, files: [{ path: tests/greet.sh, content: \"test -f greeting.txt\\n\" }], \
      tests: [{ cmd: \"sh tests/greet.sh\", proves: \"the greeting is there\" }] }, \
    { task: wave, files: [{ path: tests/wave.sh, content: \"test -f wave.txt\\n\" }], \
      tests: [{ cmd: \"sh tests/wave.sh\", proves: \"the wave is there\" }] }] }";

/// The spec proves each task's tests on the tree the task's work starts
/// from — the run's, with that task's files in it and no other's — so the
/// loop's first check of each task takes those answers instead of running
/// them again.
#[tokio::test]
async fn a_task_s_first_check_takes_what_its_spec_proved() {
    let workflow = specified_loop().replace(
        "    until: all_tasks_complete\n",
        "    until: all_tasks_complete\n    concurrency: 2\n",
    );
    let building = |task: &str, file: &str| {
        format!(
            "  - match_prompt_contains: \"{task}\"\n    effects:\n      - {{ path: {file}, content: done }}\n    outcome: {{ type: completed, summary: built }}\n"
        )
    };
    let fixture = plan_session(TWO_TASKS)
        + &specifying(&[(TWO_SPECS, true)])
        + &building("greet", "greeting.txt")
        + &building("wave", "wave.txt");
    let bench = Bench::new();

    let RunReport { terminal, .. } = bench.run(&workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    let mut reused: Vec<(String, bool)> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(p)))
                if p.phase == yunta_core::events::Phase::Pre =>
            {
                Some(p.results.clone())
            }
            _ => None,
        })
        .flatten()
        .filter(|result| result.cmd.starts_with("sh tests/"))
        .map(|result| (result.cmd, result.reused))
        .collect();
    reused.sort();
    assert_eq!(
        reused,
        vec![
            ("sh tests/greet.sh".to_string(), true),
            ("sh tests/wave.sh".to_string(), true)
        ]
    );
}

#[tokio::test]
async fn the_suite_answers_for_the_tree_before_the_tests_are_laid_over_it() {
    // A suite that goes red while a test file is in the tree without the
    // work that passes it: judged with the tests in, it would call the
    // task's criteria wrong before any session opened.
    let config = format!(
        "{MOCK_CONFIG}baseline:\n  suite: \"test ! -e tests/greet.sh || test -f greeting.txt\"\n"
    );
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_config(
            &specified_loop(),
            &building("      - { path: greeting.txt, content: Hello }\n"),
            &config,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
}

#[tokio::test]
async fn a_session_that_rewrites_its_own_test_does_not_close_its_task() {
    // Its scope reaches the tests' directory: nothing but the spec keeps
    // the file out of its work.
    let wide = PLAN.replace(
        "scope: [\"greeting.txt\"]",
        "scope: [\"greeting.txt\", \"tests/**\"]",
    );
    let bench = Bench::new();
    let RunReport { state, .. } = bench
        .run(
            &specified_loop(),
            &building_on(
                &wide,
                "      - { path: greeting.txt, content: Goodbye }
      - { path: tests/greet.sh, content: \"exit 0\\n\" }
",
            ),
        )
        .await;

    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Blocked));
    let denied: Vec<std::path::PathBuf> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::ScopeChecked(p))) => Some(p.denied.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(
        denied.contains(&std::path::PathBuf::from("tests/greet.sh")),
        "{denied:?}"
    );
}

#[tokio::test]
async fn a_gate_that_shows_a_spec_shows_its_tests() {
    let workflow = format!(
        "{WORKFLOW}  - id: approve
    kind: gate
    depends_on: [spec]
    assignee: lead
    message: \"Approve the tests?\"
    shows: [{{ node: spec, kind: spec }}]
"
    );
    let spec = spec_of("greet", GREETS, "sh tests/greet.sh");
    let fixture = plan_session(PLAN) + &specifying(&[(&spec, true)]);
    let interaction = SequencedInteraction::choosing(&["approve"]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(&workflow, &fixture, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let shown = interaction.shown();
    let yunta_engine::ShownContent::Spec(file) = &shown[0][0].content else {
        panic!("a spec is shown as its tests: {shown:?}");
    };
    assert_eq!(file.specs[0].tests[0].proves, "the greeting says hello");
}

#[tokio::test]
async fn a_plan_sent_back_from_its_gate_is_specified_again_before_the_gate_asks_again() {
    let workflow = format!(
        "{WORKFLOW}  - id: approve
    kind: gate
    depends_on: [spec]
    assignee: lead
    message: \"Approve the plan and its tests?\"
    options: [approve, adjust]
    on: {{ adjust: plan }}
    shows: [{{ node: spec, kind: spec }}]
"
    );
    let spec = spec_of("greet", GREETS, "sh tests/greet.sh");
    let fixture = plan_session(PLAN)
        + &specifying(&[(&spec, true)])
        + &tasks_session(PLAN, "replanned")
        + &specifying(&[(&spec, true)]);
    let interaction = SequencedInteraction::answering(vec![
        ("adjust", Some("greet the reader by name")),
        ("approve", None),
    ]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(&workflow, &fixture, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let specified = bench
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Artifacts(ArtifactEvent::Submitted(p)))
                    if p.artifact_kind == ArtifactKind::Spec
            )
        })
        .count();
    assert_eq!(
        specified, 2,
        "the second plan is specified before it is asked about"
    );
    assert_eq!(interaction.shown().len(), 2);
}
