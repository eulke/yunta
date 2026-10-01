//! A test a person accepts is wrong is written again by whoever wrote
//! it — nobody who builds a task writes the test that judges it — and
//! approved again before the task goes on held to it.

mod common;

use common::spec::*;
use common::*;
use yunta_core::events::{
    ArtifactEvent, EventPayload, GateEvent, NodeEvent, RerouteOrigin, TaskStatus,
};
use yunta_core::ArtifactKind;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, git_output, write, Bench};

/// The planner, the node that writes its tests, a gate that approves
/// both, and the loop that builds them.
fn approved_loop() -> String {
    format!(
        "{WORKFLOW}  - id: approve
    kind: gate
    depends_on: [spec]
    assignee: lead
    message: \"Approve the plan and its tests?\"
    shows: [{{ node: spec, kind: spec }}]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [approve]
    until: all_tasks_complete
    prompt: \"Implement your task.\"
"
    )
}

/// A test that passes once `greeting.txt` says Hi.
const SAYS_HI: &str = "test \"$(cat greeting.txt 2>/dev/null)\" = Hi\n";

/// The task session: writes a greeting that says Hi, and departs from
/// the test that wants Hello.
fn departing_from_the_test() -> String {
    "  - match_prompt_contains: \"Implement your task\"
    effects:
      - { path: greeting.txt, content: Hi }
    steps:
      - type: run_tool
        tool: yunta_declare_deviation
        arguments:
          from: { criterion: \"sh tests/greet.sh\" }
          planned: \"the greeting says Hello\"
          instead: \"it says Hi\"
          why: \"the brief asks for Hi\"
    outcome: { type: completed, summary: wrote-hi }
"
    .to_string()
}

/// The run where the test wants Hello, the task writes Hi, a person
/// accepts that, and the spec is written again as `rewritten`.
fn rewriting(rewritten: &str) -> String {
    plan_session(PLAN)
        + &specifying(&[(&spec_of("greet", GREETS, "sh tests/greet.sh"), true)])
        + &departing_from_the_test()
        + &specifying(&[(rewritten, true)])
}

/// Approves, accepts the departure saying what the test should hold,
/// and approves the tests written again.
fn approving_twice() -> SequencedInteraction {
    SequencedInteraction::answering(vec![
        ("approve", None),
        ("accept", Some("the greeting says Hi")),
        ("approve", None),
    ])
}

fn spec_submissions(bench: &Bench) -> usize {
    bench
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Artifacts(ArtifactEvent::Submitted(p)))
                    if p.artifact_kind == ArtifactKind::Spec
            )
        })
        .count()
}

#[tokio::test]
async fn a_test_a_person_accepts_departing_from_is_written_again_and_approved_before_its_task_goes_on(
) {
    let interaction = approving_twice();
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_interaction(
            &approved_loop(),
            &rewriting(&spec_of("greet", SAYS_HI, "sh tests/greet.sh")),
            &interaction,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
    assert_eq!(spec_submissions(&bench), 2, "the tests are written again");
    assert_eq!(
        interaction.shown().len(),
        3,
        "the gate, the departure, and the gate again on the new tests"
    );
    assert_eq!(
        git_output(&bench.worktree, &["show", "HEAD:tests/greet.sh"]),
        SAYS_HI.trim_end()
    );
    let rerouted = bench.events().iter().any(|event| {
        matches!(
            event.payload(),
            Some(EventPayload::Node(NodeEvent::Rerouted(p)))
                if p.to_node.as_str() == "spec" && p.origin == RerouteOrigin::GateChoice
        )
    });
    assert!(
        rerouted,
        "the loop sends the run back to whoever wrote the spec"
    );
}

#[tokio::test]
async fn the_spec_session_is_told_the_departure_and_what_the_person_said() {
    let bench = Bench::new();
    bench
        .run_with_interaction(
            &approved_loop(),
            &rewriting(&spec_of("greet", SAYS_HI, "sh tests/greet.sh")),
            &approving_twice(),
        )
        .await;

    let requests = bench.mock().requests_seen();
    let again = requests
        .iter()
        .filter(|request| request.prompt.contains("Write the tests"))
        .nth(1)
        .expect("the spec writer ran again");
    for told in [
        "task `greet` departs from the criterion `sh tests/greet.sh`",
        "its work instead it says Hi",
        "the person said: the greeting says Hi",
    ] {
        assert!(
            again.prompt.contains(told),
            "`{told}` is missing from:\n{}",
            again.prompt
        );
    }
}

#[tokio::test]
async fn a_task_that_goes_on_keeps_no_file_of_the_tests_it_left() {
    let renamed = format!(
        "{{ specs: [{{ task: greet, files: [{{ path: tests/hi.sh, content: {SAYS_HI:?} }}], \
         tests: [{{ cmd: \"sh tests/hi.sh\", proves: \"the greeting says hi\" }}] }}] }}"
    );
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_interaction(&approved_loop(), &rewriting(&renamed), &approving_twice())
        .await;

    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    let head = git_output(&bench.worktree, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(head.contains("tests/hi.sh"), "{head}");
    assert!(!head.contains("tests/greet.sh"), "{head}");
}

/// Two tasks: `hello` done first with a test that holds, then `greet`,
/// which departs from its own.
const TWO_TASKS: &str = "\
tasks:
  - id: hello
    title: \"Write hello\"
    scope: [\"hello.txt\"]
    criteria:
      - cmd: \"test -f hello.txt\"
  - id: greet
    title: \"Write the greeting\"
    scope: [\"greeting.txt\"]
    depends_on: [hello]
    criteria:
      - cmd: \"test -f greeting.txt\"
";

/// Both tasks' specs, `greet` given `greet_test`.
fn both(greet_test: &str) -> String {
    format!(
        "{{ specs: [\
         {{ task: hello, files: [{{ path: tests/hello.sh, content: \"test -f hello.txt\\n\" }}], \
           tests: [{{ cmd: \"sh tests/hello.sh\", proves: \"there is a hello\" }}] }}, \
         {{ task: greet, files: [{{ path: tests/greet.sh, content: {greet_test:?} }}], \
           tests: [{{ cmd: \"sh tests/greet.sh\", proves: \"the greeting is right\" }}] }}] }}"
    )
}

#[tokio::test]
async fn a_respecification_with_a_task_done_runs_only_the_tests_written_again() {
    let fixture = plan_session(TWO_TASKS)
        + &specifying(&[(&both(GREETS), true)])
        + "  - match_prompt_contains: \"Implement your task\"
    effects:
      - { path: hello.txt, content: hello }
    outcome: { type: completed, summary: hello }
" + &departing_from_the_test()
        + &specifying(&[(&both(SAYS_HI), true)]);
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_interaction(&approved_loop(), &fixture, &approving_twice())
        .await;

    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert!(refused(&bench).is_empty(), "{:?}", refused(&bench));
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
}

/// A spec the run did not write: a command copies it in.
const GIVEN_SPEC: &str = r#"
name: given
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: spec
    kind: bash
    depends_on: [plan]
    run: "cp given-spec.yaml {{node.artifacts}}/spec.yaml"
    artifacts:
      produces: [spec]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [spec]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

#[tokio::test]
async fn a_departure_from_a_test_nobody_in_the_run_writes_is_not_offered_for_acceptance() {
    let bench = Bench::new();
    write(
        &bench.worktree.join("given-spec.yaml"),
        &format!(
            "specs:\n  - task: greet\n    files:\n      - path: tests/greet.sh\n        \
             content: {GREETS:?}\n    tests:\n      - cmd: sh tests/greet.sh\n        \
             proves: the greeting says hello\n"
        ),
    );
    git(&bench.worktree, &["add", "given-spec.yaml"]);
    git(
        &bench.worktree,
        &["commit", "-q", "-m", "a spec of the project's"],
    );
    let fixture = plan_session(PLAN) + &departing_from_the_test();
    bench
        .run_with_interaction(
            GIVEN_SPEC,
            &fixture,
            &SequencedInteraction::choosing(&["abort"]),
        )
        .await;

    let offered: Vec<String> = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::Waiting(p))) => Some(
                p.options()
                    .iter()
                    .map(|option| option.id.to_string())
                    .collect(),
            ),
            _ => None,
        })
        .expect("the departure was put to a person");
    assert_eq!(offered, ["send-back", "abort"]);
}

#[tokio::test]
async fn a_run_paused_before_the_tests_written_again_are_approved_goes_on_when_resumed() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(
            &approved_loop(),
            &rewriting(&spec_of("greet", SAYS_HI, "sh tests/greet.sh")),
            &SequencedInteraction::answering(vec![("approve", None), ("accept", None)]),
        )
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "nobody approved the tests written again: {terminal:?}"
    );

    let RunReport { terminal, state } = bench
        .wake_answering(&SequencedInteraction::choosing(&["approve"]))
        .await;

    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
    assert_eq!(
        spec_submissions(&bench),
        2,
        "written again once, not on the wake"
    );
}

#[tokio::test]
async fn a_spec_written_again_that_changes_another_tasks_spec_is_refused_naming_it() {
    let rewrites_hello = both(SAYS_HI).replace("test -f hello.txt", "test -s hello.txt");
    let fixture = plan_session(TWO_TASKS)
        + &specifying(&[(&both(GREETS), true)])
        + "  - match_prompt_contains: \"Implement your task\"
    effects:
      - { path: hello.txt, content: hello }
    outcome: { type: completed, summary: hello }
" + &departing_from_the_test()
        + &specifying(&[(&rewrites_hello, false), (&both(SAYS_HI), true)]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(&approved_loop(), &fixture, &approving_twice())
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(refused(&bench), [vec!["other-spec-changed".to_string()]]);
}

#[tokio::test]
async fn a_spec_written_again_that_gives_the_task_the_same_tests_is_refused() {
    let same = spec_of("greet", GREETS, "sh tests/greet.sh");
    let fixture = plan_session(PLAN)
        + &specifying(&[(&same, true)])
        + &departing_from_the_test()
        + &specifying(&[
            (&same, false),
            (&spec_of("greet", SAYS_HI, "sh tests/greet.sh"), true),
        ]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(&approved_loop(), &fixture, &approving_twice())
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        refused(&bench),
        [vec!["departed-spec-unchanged".to_string()]]
    );
}
