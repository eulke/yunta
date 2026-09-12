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
    diagnostics.iter().map(Diagnostic::code).collect()
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
        scope: scope.iter().map(|s| s.to_string()).collect(),
        criteria,
        depends_on: depends_on.iter().map(|&d| d.into()).collect(),
        notes: None,
        manual_review: false,
        justification: None,
    }
}

fn cmd(cmd: &str) -> Criterion {
    Criterion {
        cmd: cmd.to_string(),
        r#type: None,
    }
}

fn guard(cmd: &str) -> Criterion {
    Criterion {
        cmd: cmd.to_string(),
        r#type: Some(CriterionType::Guard),
    }
}

#[test]
fn a_well_formed_tasks_document_has_no_errors() {
    let tasks = TasksFile {
        tasks: vec![
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
        ],
    };
    assert_eq!(check(&tasks), Vec::new());
}

#[test]
fn duplicate_id_is_reported() {
    let tasks = TasksFile {
        tasks: vec![
            task("a", &["src/a/**"], vec![cmd("true")], &[]),
            task("a", &["src/b/**"], vec![cmd("true")], &[]),
        ],
    };
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["duplicate-id"]);
    assert!(rendered(&errors).contains("task `a`"));
}

#[test]
fn unknown_dependency_is_reported() {
    let tasks = TasksFile {
        tasks: vec![task("a", &["src/**"], vec![cmd("true")], &["ghost"])],
    };
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["unknown-dependency"]);
    assert!(rendered(&errors).contains("`ghost`"));
}

#[test]
fn dependency_cycle_is_reported() {
    let tasks = TasksFile {
        tasks: vec![
            task("a", &["src/a/**"], vec![cmd("true")], &["b"]),
            task("b", &["src/b/**"], vec![cmd("true")], &["a"]),
        ],
    };
    let errors = check(&tasks);
    assert!(codes(&errors).contains(&"dependency-cycle"));
}

#[test]
fn empty_scope_is_reported() {
    let tasks = TasksFile {
        tasks: vec![task("a", &[], vec![cmd("true")], &[])],
    };
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["empty-scope"]);
}

#[test]
fn empty_title_is_reported() {
    let mut tasks = TasksFile {
        tasks: vec![task("a", &["src/**"], vec![cmd("true")], &[])],
    };
    tasks.tasks[0].title = "   ".to_string();
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["empty-title"]);
}

#[test]
fn no_criteria_is_reported() {
    let tasks = TasksFile {
        tasks: vec![task("a", &["src/**"], vec![], &[])],
    };
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["no-criteria"]);
}

#[test]
fn all_criteria_being_guards_is_reported() {
    let tasks = TasksFile {
        tasks: vec![task(
            "a",
            &["src/**"],
            vec![guard("cargo clippy --workspace -- -D warnings")],
            &[],
        )],
    };
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["all-criteria-are-guards"]);
}

#[test]
fn a_guard_alongside_a_real_criterion_is_fine() {
    let tasks = TasksFile {
        tasks: vec![task(
            "a",
            &["src/**"],
            vec![
                cmd("cargo test -p yunta"),
                guard("cargo clippy --workspace -- -D warnings"),
            ],
            &[],
        )],
    };
    assert_eq!(check(&tasks), Vec::new());
}

#[test]
fn manual_review_without_justification_is_reported() {
    let mut tasks = TasksFile {
        tasks: vec![task("a", &["src/**"], vec![cmd("true")], &[])],
    };
    tasks.tasks[0].manual_review = true;
    let errors = check(&tasks);
    assert_eq!(codes(&errors), ["manual-review-without-justification"]);
}

#[test]
fn manual_review_with_justification_is_fine() {
    let mut tasks = TasksFile {
        tasks: vec![task("a", &["docs/**"], vec![cmd("true")], &[])],
    };
    tasks.tasks[0].manual_review = true;
    tasks.tasks[0].justification = Some("prose review, no command can verify tone".to_string());
    assert_eq!(check(&tasks), Vec::new());
}

#[test]
fn overlapping_scopes_without_a_dependency_are_reported() {
    let tasks = TasksFile {
        tasks: vec![
            task("a", &["src/**"], vec![cmd("true")], &[]),
            task("b", &["src/lib.rs"], vec![cmd("true")], &[]),
        ],
    };
    let errors = check(&tasks);
    assert!(codes(&errors).contains(&"overlapping-scope"));
}

#[test]
fn overlapping_scopes_with_a_dependency_between_them_are_fine() {
    let tasks = TasksFile {
        tasks: vec![
            task("a", &["src/**"], vec![cmd("true")], &[]),
            task("b", &["src/lib.rs"], vec![cmd("true")], &["a"]),
        ],
    };
    assert_eq!(check(&tasks), Vec::new());
}

#[test]
fn disjoint_scopes_never_get_flagged() {
    let tasks = TasksFile {
        tasks: vec![
            task("a", &["crates/core/**"], vec![cmd("true")], &[]),
            task("b", &["crates/cli/**"], vec![cmd("true")], &[]),
        ],
    };
    assert_eq!(check(&tasks), Vec::new());
}

#[test]
fn every_violation_names_the_task_the_field_and_what_to_do() {
    let tasks = TasksFile {
        tasks: vec![
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
        ],
    };
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
        serde_norway::from_str(yaml).expect("reference tasks document should parse");
    assert_eq!(tasks.tasks.len(), 2);
    // Overlapping scope with its own dependency ancestor is fine; the
    // registration should be clean.
    assert_eq!(check(&tasks), Vec::new());
}
