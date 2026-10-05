//! What a tasks document is held to as part of the run it is handed
//! over in: what proves its work, the spec the run writes or holds, and
//! the questions a person answered.

use yunta_core::TasksFile;

/// The codes of `diagnostics`, in order.
fn codes(diagnostics: &[yunta_core::diagnostic::Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.problem.code().to_string())
        .collect()
}

/// A plan with the shape of one a planner wrote to get past the rule that
/// a criterion fails before the work: each task's criterion passes once a
/// test's name is written, one of them in the test file the task changes.
const PROVEN_BY_A_NAME: &str = r#"
tasks:
  - id: cli
    title: CLI
    scope: [src/pack.rs, tests/pack_cmd.rs]
    criteria:
      - { cmd: "cargo test --test pack_cmd && grep -q 'scope_case' tests/pack_cmd.rs" }
    changes:
      - { at: src/pack.rs::PackStore, what: the store }
      - { at: tests/pack_cmd.rs, what: lifecycle tests }
  - id: engine
    title: Engine
    scope: [src/catalog.rs]
    criteria:
      - { cmd: "cargo test --lib && grep -q 'resolution_case' src/catalog.rs" }
    changes:
      - { at: src/catalog.rs::PackRoots, what: the roots }
"#;

#[test]
fn a_plan_proven_by_a_name_is_refused_and_writing_its_own_test_only_where_a_spec_writes_them() {
    let tasks: TasksFile = yunta_core::yaml::parse(PROVEN_BY_A_NAME).unwrap();
    assert_eq!(
        codes(&tasks.unspecifiable(true, None)),
        [
            "criterion-checks-presence",
            "task-writes-its-test",
            "criterion-checks-presence"
        ]
    );
    assert_eq!(
        codes(&tasks.unspecifiable(false, None)),
        ["criterion-checks-presence", "criterion-checks-presence"],
        "without a spec, a task writes its own tests"
    );
}

#[test]
fn two_tasks_judged_by_one_command_and_a_task_using_its_own_shape_are_refused() {
    let tasks: TasksFile = yunta_core::yaml::parse(
        r#"
shapes:
  - { name: Store, owner: a, file: src/a.rs, code: "pub struct Store;" }
tasks:
  - id: a
    title: A
    scope: [src/a.rs]
    uses: [Store]
    criteria: [{ cmd: "cargo test --test a" }]
  - id: b
    title: B
    scope: [src/b.rs]
    depends_on: [a]
    criteria: [{ cmd: "cargo test --test a" }]
"#,
    )
    .unwrap();
    assert_eq!(
        codes(&tasks.unspecifiable(false, None)),
        ["uses-its-own-shape", "shared-criterion"]
    );
}

#[test]
fn a_change_on_a_file_the_spec_wrote_is_refused() {
    let tasks: TasksFile = yunta_core::yaml::parse(PROVEN_BY_A_NAME).unwrap();
    let spec: yunta_core::SpecFile = yunta_core::yaml::parse(
        r##"
specs:
  - task: cli
    files: [{ path: ./tests/pack_cmd.rs, content: "#[test] fn t() {}" }]
    tests: [{ cmd: "cargo test --test pack_cmd", proves: p }]
"##,
    )
    .unwrap();
    assert_eq!(codes(&tasks.against_spec(&spec)), ["changes-a-spec-test"]);
}

#[test]
fn a_decision_answers_only_a_question_a_person_answered() {
    let tasks: TasksFile = yunta_core::yaml::parse(
        r#"
decisions:
  - { id: where, question: "Where?", choice: "user state", answers: install-scope }
  - { id: how, question: "How?", choice: "a flag", answers: global-command }
tasks:
  - { id: a, title: A, scope: [a], criteria: [{ cmd: "false" }] }
"#,
    )
    .unwrap();
    let answered = [yunta_core::QuestionId::from("install-scope")];
    let refused = tasks.unanswered(&answered);
    assert_eq!(codes(&refused), ["unknown-answer"]);
    assert!(
        refused[0].to_string().contains("global-command"),
        "{}",
        refused[0]
    );
}

/// A plan whose tasks run the project's tests three ways: one package on
/// its own, the suite extended, and a program of their own.
const RUN_THREE_WAYS: &str = r#"
tasks:
  - id: package
    title: Package
    scope: [src/a.rs]
    criteria: [{ cmd: "cargo build && cargo test -p engine --test retry" }]
  - id: extended
    title: Extended
    scope: [src/b.rs]
    criteria: [{ cmd: "cargo test --workspace --test pool" }]
  - id: script
    title: Script
    scope: [src/c.rs]
    criteria: [{ cmd: "sh tests/c.sh" }, { cmd: "cargo clippy -p engine", type: guard }]
"#;

#[test]
fn a_package_criterion_outside_the_suite_is_refused_naming_it() {
    let tasks: TasksFile = yunta_core::yaml::parse(RUN_THREE_WAYS).unwrap();
    let suite = yunta_core::Suite::of("cargo test --workspace").unwrap();

    let broken = tasks.unspecifiable(false, Some(&suite));

    assert_eq!(codes(&broken), ["outside-the-suite"]);
    let said = broken[0].to_string();
    assert!(
        said.contains("`cargo test -p engine --test retry`")
            && said.contains("`cargo test --workspace` followed by"),
        "{said}"
    );
}

#[test]
fn a_criterion_extending_the_suite_or_running_another_program_is_accepted() {
    let tasks: TasksFile = yunta_core::yaml::parse(RUN_THREE_WAYS).unwrap();
    let suite = yunta_core::Suite::of("cargo test --workspace").unwrap();

    let refused: Vec<String> = tasks
        .unspecifiable(false, Some(&suite))
        .iter()
        .map(|diagnostic| diagnostic.to_string())
        .collect();

    assert!(
        refused.iter().all(|said| said.contains("`package`")),
        "only the package task is held: {refused:?}"
    );
}
