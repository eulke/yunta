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
