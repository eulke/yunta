//! The rules a tasks document has to satisfy once it is readable, each asserted
//! by the code it reports rather than by its wording: the code is what a
//! receipt counts and a log is searched by, so it is the part that has
//! to stay put while the sentence is free to improve.

use yunta_core::diagnostic::Diagnostic;
use yunta_core::events::CriterionType;
use yunta_core::shape::Document;
use yunta_core::Criterion;
use yunta_core::{Task, TasksFile};

/// Every violation the tasks document carries, as `shape::read` asks for them.
fn check(tasks: &TasksFile) -> Vec<Diagnostic> {
    tasks.check()
}

/// The stable name of every rule a tasks document broke, in the order reported.
/// Asserting on these rather than on a variant is deliberate: the code
/// is what a receipt counts and a log is searched by, so it is the part
/// that has to stay put while the wording is free to improve.
fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|d| d.code().as_str()).collect()
}

/// Every violation as a reader sees it, joined — what the assertions
/// about naming and wording read.
fn rendered(diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn task(id: &str, scope: &[&str], criteria: Vec<Criterion>, depends_on: &[&str]) -> Task {
    Task {
        id: id.into(),
        title: format!("do {id}"),
        scope: scope.iter().map(|s| (*s).into()).collect(),
        criteria,
        depends_on: depends_on.iter().map(|&d| d.into()).collect(),
        notes: None,
        description: None,
        changes: Vec::new(),
        outcome: None,
        uses: Vec::new(),
        invariants: Vec::new(),
    }
}

fn cmd(cmd: &str) -> Criterion {
    Criterion {
        cmd: cmd.to_string(),
        r#type: None,
        proves: None,
    }
}

fn guard(cmd: &str) -> Criterion {
    Criterion {
        cmd: cmd.to_string(),
        r#type: Some(CriterionType::Guard),
        proves: None,
    }
}

#[test]
fn a_well_formed_tasks_document_has_no_errors() {
    let tasks = TasksFile::of(vec![
        task(
            "context-sources",
            &["crates/engine/src/context/**"],
            vec![cmd("cargo test -p yunta-engine context::")],
            &[],
        ),
        task(
            "context-assembly",
            &["crates/engine/src/context/other/**"],
            vec![cmd("cargo test -p yunta-engine --test context_stability")],
            &["context-sources"],
        ),
    ]);
    assert_eq!(check(&tasks), Vec::new());
}

#[test]
fn duplicate_id_is_reported() {
    let tasks = TasksFile::of(vec![
        task("a", &["src/a/**"], vec![cmd("true")], &[]),
        task("a", &["src/b/**"], vec![cmd("true")], &[]),
    ]);
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["duplicate-id"]);
    assert!(rendered(&errors).contains("task `a`"));
}

#[test]
fn unknown_dependency_is_reported() {
    let tasks = TasksFile::of(vec![task("a", &["src/**"], vec![cmd("true")], &["ghost"])]);
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["unknown-dependency"]);
    assert!(rendered(&errors).contains("`ghost`"));
}

#[test]
fn dependency_cycle_is_reported() {
    let tasks = TasksFile::of(vec![
        task("a", &["src/a/**"], vec![cmd("true")], &["b"]),
        task("b", &["src/b/**"], vec![cmd("true")], &["a"]),
    ]);
    let errors = check(&tasks);
    assert!(codes(&errors).contains(&"dependency-cycle"));
}

#[test]
fn empty_scope_is_reported() {
    let tasks = TasksFile::of(vec![task("a", &[], vec![cmd("true")], &[])]);
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["empty-scope"]);
}

#[test]
fn empty_title_is_reported() {
    let mut tasks = TasksFile::of(vec![task("a", &["src/**"], vec![cmd("true")], &[])]);
    tasks.tasks[0].title = "   ".to_string();
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["empty-title"]);
}

#[test]
fn no_criteria_is_reported() {
    let tasks = TasksFile::of(vec![task("a", &["src/**"], vec![], &[])]);
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["no-criteria"]);
}

#[test]
fn all_criteria_being_guards_is_reported() {
    let tasks = TasksFile::of(vec![task(
        "a",
        &["src/**"],
        vec![guard("cargo clippy --workspace -- -D warnings")],
        &[],
    )]);
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["all-criteria-are-guards"]);
}

#[test]
fn a_guard_alongside_a_real_criterion_is_fine() {
    let tasks = TasksFile::of(vec![task(
        "a",
        &["src/**"],
        vec![
            cmd("cargo test -p yunta"),
            guard("cargo clippy --workspace -- -D warnings"),
        ],
        &[],
    )]);
    assert_eq!(check(&tasks), Vec::new());
}

#[test]
fn overlapping_scopes_without_a_dependency_are_reported() {
    let tasks = TasksFile::of(vec![
        task("a", &["src/**"], vec![cmd("true")], &[]),
        task("b", &["src/lib.rs"], vec![cmd("true")], &[]),
    ]);
    let errors = check(&tasks);
    assert!(codes(&errors).contains(&"overlapping-scope"));
}

#[test]
fn overlapping_scopes_with_a_dependency_between_them_are_fine() {
    let tasks = TasksFile::of(vec![
        task("a", &["src/**"], vec![cmd("true")], &[]),
        task("b", &["src/lib.rs"], vec![cmd("true")], &["a"]),
    ]);
    assert_eq!(check(&tasks), Vec::new());
}

#[test]
fn disjoint_scopes_never_get_flagged() {
    let tasks = TasksFile::of(vec![
        task("a", &["crates/core/**"], vec![cmd("true")], &[]),
        task("b", &["crates/cli/**"], vec![cmd("true")], &[]),
    ]);
    assert_eq!(check(&tasks), Vec::new());
}

#[test]
fn every_violation_names_the_task_the_field_and_what_to_do() {
    let tasks = TasksFile::of(vec![
        task("graph-cmd", &[], vec![cmd("true")], &[]),
        task(
            "parse-events",
            &["src/**"],
            vec![cmd("true")],
            &["storage-init"],
        ),
        task(
            "T004",
            &["docs/**"],
            vec![guard("cargo clippy --workspace -- -D warnings")],
            &[],
        ),
    ]);
    let errors = check(&tasks);
    let text = rendered(&errors);
    assert!(
        text.contains("task `graph-cmd`: `scope` is empty; every task declares at least one glob"),
        "{text}"
    );
    assert!(
        text.contains(
            "task `parse-events`: `depends_on` names `storage-init`, which no task in this \
             file declares"
        ),
        "{text}"
    );
    assert!(
        text.contains("task `T004`: every criterion is a `guard`"),
        "{text}"
    );
}

#[test]
fn parses_the_reference_tasks_document_from_the_spec() {
    let yaml = r#"
tasks:
  - id: context-sources
    title: "ContextSource trait with files, command and artifact builtins"
    scope: ["crates/engine/src/context/**"]
    criteria:
      - cmd: "cargo test -p yunta-engine context::"
      - cmd: "! grep -rn 'todo!()' crates/engine/src/context/"
      - cmd: "cargo clippy --workspace -- -D warnings"
        type: guard
    notes: "Materializar en context/<hash>/; fuente caída = nodo failed."

  - id: context-assembly
    title: "Stable-first context assembly with per-segment hashes"
    depends_on: [context-sources]
    scope: ["crates/engine/src/context/**", "crates/core/src/events.rs"]
    criteria:
      - cmd: "cargo test -p yunta-engine --test context_stability"
      - cmd: "cargo clippy --workspace -- -D warnings"
        type: guard
    notes: "Ver Contrato del Run."
"#;
    let tasks: TasksFile =
        yunta_core::yaml::parse(yaml).expect("reference tasks document should parse");
    assert_eq!(tasks.tasks.len(), 2);
    // Overlapping scope with its own dependency ancestor is fine; the
    // registration should be clean.
    assert_eq!(check(&tasks), Vec::new());
}

/// The codes a plan's missing explanation is reported under, in order.
fn unexplained_codes(tasks: &TasksFile) -> Vec<String> {
    tasks
        .unexplained()
        .iter()
        .map(|diagnostic| match &diagnostic.problem {
            yunta_core::diagnostic::Problem::Rule { code, .. } => code.to_string(),
            other => panic!("a missing explanation is a broken rule, got {other:?}"),
        })
        .collect()
}

#[test]
fn a_plan_a_person_reviews_names_every_piece_of_its_explanation_it_lacks() {
    let tasks = TasksFile::of(vec![task(
        "a",
        &["src/a.rs"],
        vec![cmd("test -f src/a.rs"), guard("true")],
        &[],
    )]);
    assert_eq!(
        unexplained_codes(&tasks),
        [
            "no-summary",
            "no-description",
            "no-description",
            "no-outcome",
            "no-changes",
            "unexplained-criterion",
            "unexplained-criterion"
        ],
        "the plan's, the task's, and each criterion's"
    );
}

#[test]
fn a_plan_that_creates_no_shape_and_risks_nothing_is_still_explained() {
    let tasks: TasksFile = yunta_core::yaml::parse(
        r#"
summary: "Write the greeting"
description: "The project greets whoever opens it."
tasks:
  - id: a
    title: "Write it"
    description: "Adds hello.txt."
    scope: [hello.txt]
    changes: [{ at: hello.txt, what: "the greeting" }]
    outcome: "Opening the project greets you"
    criteria: [{ cmd: "test -f hello.txt", proves: "the greeting exists" }]
"#,
    )
    .unwrap();
    assert_eq!(
        unexplained_codes(&tasks),
        Vec::<String>::new(),
        "`design`, `decisions`, `shapes`, `risks` and `out_of_scope` are the prompt's to \
         ask for"
    );
}

#[test]
fn the_published_example_is_a_plan_a_person_can_review() {
    let example: TasksFile =
        yunta_core::shape::read(TasksFile::EXAMPLE.as_bytes(), "example").unwrap();
    assert_eq!(unexplained_codes(&example), Vec::<String>::new());
}

// --- shapes, decisions and changes ------------------------------------------

/// The codes `yaml` breaks, as `check` reports them.
fn broken_codes(yaml: &str) -> Vec<String> {
    let tasks: TasksFile = yunta_core::yaml::parse(yaml).unwrap();
    check(&tasks)
        .iter()
        .map(|diagnostic| match &diagnostic.problem {
            yunta_core::diagnostic::Problem::Rule { code, .. } => code.to_string(),
            other => panic!("a broken rule, got {other:?}"),
        })
        .collect()
}

/// A palette one task builds and another uses, the way the plan the
/// rules exist for declared it.
const PALETTE: &str = r#"
shapes:
  - name: ColorRole
    owner: color-policy
    file: crates/cli/src/render/color.rs
    code: "pub enum ColorRole { Error, Warning, Success, Info }"
tasks:
  - id: color-policy
    title: Add the palette
    scope: [crates/cli/src/render/color.rs]
    criteria: [{ cmd: "test -f crates/cli/src/render/color.rs" }]
  - id: color-command-output
    title: Color the reports
    scope: ["crates/cli/src/commands/**"]
    depends_on: [color-policy]
    uses: [ColorRole]
    criteria: [{ cmd: "test -f crates/cli/src/commands/colored" }]
"#;

#[test]
fn a_shape_its_owner_builds_and_another_task_waits_for_breaks_nothing() {
    assert_eq!(broken_codes(PALETTE), Vec::<String>::new());
}

#[test]
fn a_change_in_a_file_the_task_may_not_touch_is_refused() {
    // The report task needed a role the palette lacked, in a file only
    // the palette's task may write: declared, it is refused at once.
    let yaml = format!(
        "{PALETTE}    changes:\n      - {{ at: \"crates/cli/src/render/color.rs::ColorRole\", what: \"add Success\" }}\n"
    );
    assert_eq!(broken_codes(&yaml), ["change-outside-scope"]);
}

#[test]
fn a_task_that_uses_a_shape_without_waiting_for_its_owner_is_refused() {
    let yaml = PALETTE.replace("    depends_on: [color-policy]\n", "");
    assert!(
        broken_codes(&yaml).contains(&"shape-used-before-its-owner".to_string()),
        "{:?}",
        broken_codes(&yaml)
    );
}

#[test]
fn a_shape_whose_owner_may_not_write_its_file_is_refused() {
    let yaml = PALETTE.replace(
        "file: crates/cli/src/render/color.rs",
        "file: crates/cli/src/render/theme.rs",
    );
    assert_eq!(broken_codes(&yaml), ["shape-outside-owner-scope"]);
}

#[test]
fn a_shape_owned_or_used_by_nobody_declared_is_refused() {
    let unowned = PALETTE.replace("owner: color-policy", "owner: palette");
    assert!(broken_codes(&unowned).contains(&"unknown-shape-owner".to_string()));
    let unknown = PALETTE.replace("uses: [ColorRole]", "uses: [ColourRole]");
    assert_eq!(broken_codes(&unknown), ["unknown-shape"]);
}

#[test]
fn a_shape_or_a_decision_declared_twice_is_refused() {
    let twice = PALETTE.replace(
        "tasks:\n",
        "  - name: ColorRole\n    owner: color-policy\n    file: crates/cli/src/render/color.rs\n    code: x\ntasks:\n",
    );
    assert_eq!(broken_codes(&twice), ["duplicate-shape"]);
    let decided = format!(
        "decisions:\n  - {{ id: palette, question: q, choice: a, why: w }}\n  - {{ id: palette, question: q, choice: b, why: w }}\n{PALETTE}"
    );
    assert_eq!(broken_codes(&decided), ["duplicate-decision"]);
}

#[test]
fn a_decision_a_person_reviews_says_why() {
    let tasks: TasksFile = yunta_core::yaml::parse(
        r#"
summary: s
description: d
decisions:
  - { id: palette, question: "Which colors?", choice: "The terminal's 16" }
tasks:
  - id: a
    title: t
    description: d
    scope: [a.txt]
    changes: [{ at: a.txt, what: w }]
    outcome: o
    criteria: [{ cmd: "test -f a.txt", proves: p }]
"#,
    )
    .unwrap();
    assert_eq!(unexplained_codes(&tasks), ["unexplained-decision"]);
}
