use yunta_core::events::CriterionType;
use yunta_core::Criterion;
use yunta_core::{Ledger, Task};
use yunta_engine::{register, LedgerError};

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
fn a_well_formed_ledger_has_no_errors() {
    let ledger = Ledger {
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
    assert_eq!(register(&ledger), Vec::new());
}

#[test]
fn duplicate_id_is_reported() {
    let ledger = Ledger {
        tasks: vec![
            task("a", &["src/a/**"], vec![cmd("true")], &[]),
            task("a", &["src/b/**"], vec![cmd("true")], &[]),
        ],
    };
    let errors = register(&ledger);
    assert_eq!(errors, vec![LedgerError::DuplicateId { id: "a".into() }]);
}

#[test]
fn unknown_dependency_is_reported() {
    let ledger = Ledger {
        tasks: vec![task("a", &["src/**"], vec![cmd("true")], &["ghost"])],
    };
    let errors = register(&ledger);
    assert_eq!(
        errors,
        vec![LedgerError::UnknownDependency {
            task: "a".into(),
            unknown: "ghost".into(),
        }]
    );
}

#[test]
fn dependency_cycle_is_reported() {
    let ledger = Ledger {
        tasks: vec![
            task("a", &["src/a/**"], vec![cmd("true")], &["b"]),
            task("b", &["src/b/**"], vec![cmd("true")], &["a"]),
        ],
    };
    let errors = register(&ledger);
    assert!(errors
        .iter()
        .any(|e| matches!(e, LedgerError::DependencyCycle { .. })));
}

#[test]
fn empty_scope_is_reported() {
    let ledger = Ledger {
        tasks: vec![task("a", &[], vec![cmd("true")], &[])],
    };
    let errors = register(&ledger);
    assert_eq!(errors, vec![LedgerError::EmptyScope { task: "a".into() }]);
}

#[test]
fn empty_title_is_reported() {
    let mut ledger = Ledger {
        tasks: vec![task("a", &["src/**"], vec![cmd("true")], &[])],
    };
    ledger.tasks[0].title = "   ".to_string();
    let errors = register(&ledger);
    assert_eq!(errors, vec![LedgerError::EmptyTitle { task: "a".into() }]);
}

#[test]
fn no_criteria_is_reported() {
    let ledger = Ledger {
        tasks: vec![task("a", &["src/**"], vec![], &[])],
    };
    let errors = register(&ledger);
    assert_eq!(errors, vec![LedgerError::NoCriteria { task: "a".into() }]);
}

#[test]
fn all_criteria_being_guards_is_reported() {
    let ledger = Ledger {
        tasks: vec![task(
            "a",
            &["src/**"],
            vec![guard("cargo clippy --workspace -- -D warnings")],
            &[],
        )],
    };
    let errors = register(&ledger);
    assert_eq!(
        errors,
        vec![LedgerError::AllCriteriaAreGuards { task: "a".into() }]
    );
}

#[test]
fn a_guard_alongside_a_real_criterion_is_fine() {
    let ledger = Ledger {
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
    assert_eq!(register(&ledger), Vec::new());
}

#[test]
fn manual_review_without_justification_is_reported() {
    let mut ledger = Ledger {
        tasks: vec![task("a", &["src/**"], vec![cmd("true")], &[])],
    };
    ledger.tasks[0].manual_review = true;
    let errors = register(&ledger);
    assert_eq!(
        errors,
        vec![LedgerError::ManualReviewWithoutJustification { task: "a".into() }]
    );
}

#[test]
fn manual_review_with_justification_is_fine() {
    let mut ledger = Ledger {
        tasks: vec![task("a", &["docs/**"], vec![cmd("true")], &[])],
    };
    ledger.tasks[0].manual_review = true;
    ledger.tasks[0].justification = Some("prose review, no command can verify tone".to_string());
    assert_eq!(register(&ledger), Vec::new());
}

#[test]
fn overlapping_scopes_without_a_dependency_are_reported() {
    let ledger = Ledger {
        tasks: vec![
            task("a", &["src/**"], vec![cmd("true")], &[]),
            task("b", &["src/lib.rs"], vec![cmd("true")], &[]),
        ],
    };
    let errors = register(&ledger);
    assert!(errors
        .iter()
        .any(|e| matches!(e, LedgerError::OverlappingScope { .. })));
}

#[test]
fn overlapping_scopes_with_a_dependency_between_them_are_fine() {
    let ledger = Ledger {
        tasks: vec![
            task("a", &["src/**"], vec![cmd("true")], &[]),
            task("b", &["src/lib.rs"], vec![cmd("true")], &["a"]),
        ],
    };
    assert_eq!(register(&ledger), Vec::new());
}

#[test]
fn disjoint_scopes_never_get_flagged() {
    let ledger = Ledger {
        tasks: vec![
            task("a", &["crates/core/**"], vec![cmd("true")], &[]),
            task("b", &["crates/cli/**"], vec![cmd("true")], &[]),
        ],
    };
    assert_eq!(register(&ledger), Vec::new());
}

#[test]
fn every_error_message_names_the_task_and_the_rule() {
    assert_eq!(
        LedgerError::EmptyScope {
            task: "graph-cmd".into()
        }
        .to_string(),
        "graph-cmd: `scope` is empty — every task must declare at least one glob"
    );
    assert_eq!(
        LedgerError::UnknownDependency {
            task: "parse-events".into(),
            unknown: "storage-init".into()
        }
        .to_string(),
        "parse-events: `depends_on` references unknown task `storage-init`"
    );
    assert_eq!(
        LedgerError::AllCriteriaAreGuards {
            task: "T004".into()
        }
        .to_string(),
        "T004: all criteria are `guard` — at least one must be able to fail before the work"
    );
}

#[test]
fn parses_the_reference_ledger_from_the_spec() {
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
    let ledger: Ledger = serde_yaml::from_str(yaml).expect("reference ledger should parse");
    assert_eq!(ledger.tasks.len(), 2);
    // Overlapping scope with its own dependency ancestor is fine; the
    // registration should be clean.
    assert_eq!(register(&ledger), Vec::new());
}
