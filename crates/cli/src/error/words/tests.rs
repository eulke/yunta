use super::{check, close, inputs, project_view, refused_answer, resolve_gate};
use yunta_core::events::{Claim, ObjectProblem, Refusal, WhyWithheld, Withheld};
use yunta_core::shown::Flaw;
use yunta_core::{sha256_hex, ArtifactSpec, ConfigKey, NonEmpty, OptionId, TaskId};
use yunta_engine::{
    CheckError, CloseRunError, Composition, InputsError, ResolveGateError, RunError, StoreError,
    Unanswerable, UntakenRoute,
};

/// `proceed`, withheld because the plan it would take the run past cannot be
/// proven: its one flaw is a spec test that runs none of the spec's files.
fn withheld_proceed() -> Withheld {
    Withheld {
        option: OptionId::from_static("proceed"),
        because: WhyWithheld::Unprovable {
            flaws: NonEmpty::from((
                Flaw::HollowSpecTest {
                    task: TaskId::from_static("T001"),
                    cmd: "test -f a.txt".to_string(),
                },
                Vec::new(),
            )),
        },
    }
}

#[test]
fn every_error_message_names_its_rule() {
    assert_eq!(
        check(&CheckError::DependsOnCycle {
            cycle: vec!["a".into(), "b".into(), "a".into()]
        }),
        "cycle in depends_on: a -> b -> a"
    );
    assert_eq!(
        check(&CheckError::UnknownRunner {
            node: "a".into(),
            runner: "planner".into(),
            near: None,
        }),
        "node `a` references runner `planner`, which `runners:` does not define"
    );
    assert_eq!(
        check(&CheckError::UnknownRunner {
            node: "a".into(),
            runner: "planer".into(),
            near: Some("planner".into()),
        }),
        "node `a` references runner `planer`, which `runners:` does not define — did you mean \
         `planner`?"
    );
}

#[test]
fn the_refusal_spells_the_split_and_how_to_read_the_answers() {
    let refusal = check(&CheckError::QuestionsAlongsideOtherArtifacts {
        node: "grill".into(),
        others: vec![ArtifactSpec::Opaque("brief.md".to_string())],
    });
    assert!(
        refusal.contains("`brief.md`"),
        "the refusal names what has to move: {refusal}"
    );
    assert!(
        refusal.contains("a node that follows it"),
        "the refusal says where it goes: {refusal}"
    );
    assert!(
        refusal.contains("kind: answers"),
        "the refusal spells how the next node reads the answers: {refusal}"
    );
}

#[test]
fn a_node_cannot_owe_the_answers_the_engine_writes() {
    let refusal = check(&CheckError::AnswersDeclaredAsProduced { node: "a".into() });
    assert!(
        refusal.contains("kind: answers"),
        "the refusal spells how the answers are read instead: {refusal}"
    );
}

#[test]
fn an_input_document_and_a_node_producing_its_kind_are_refused_together() {
    let clash = CheckError::InputDocumentAlsoProduced {
        input: "plan".into(),
        node: "plan-it".into(),
        kind: yunta_core::ArtifactKind::Tasks,
    };
    let text = check(&clash);
    assert!(
        text.contains("`plan`") && text.contains("`plan-it`") && text.contains("tasks document"),
        "the refusal names both producers and the document they claim: {text}"
    );
}

#[test]
fn a_gate_that_routes_abort_somewhere_is_reported() {
    assert_eq!(
        check(&CheckError::GateRouteNeverTaken {
            node: "approve".into(),
            route: UntakenRoute::Abort,
        }),
        "gate `approve`: `on.abort` is never taken — choosing `abort` pauses the run; give the \
         option that sends the run back another id"
    );
}

#[test]
fn independent_nodes_with_overlapping_scope_error_under_parallel_fanout() {
    let text = check(&CheckError::OverlappingFanOutScope {
        a: "a".into(),
        b: "b".into(),
        glob_a: "src/shared.rs".parse().unwrap(),
        glob_b: "src/shared.rs".parse().unwrap(),
    });
    assert!(
        text.contains("`a`") && text.contains("`b`"),
        "the error must cite both nodes: {text}"
    );
}

#[test]
fn a_yunta_schema_range_covering_this_binary_passes_and_one_outside_fails() {
    let text = check(&CheckError::YuntaSchemaOutside {
        range: ">=2".parse().unwrap(),
        binary: 1,
    });
    assert!(
        text.contains("yunta_schema"),
        "an out-of-range requirement names its key: {text}"
    );
}

#[test]
fn a_distill_path_no_node_declares_producing_fails_check() {
    let text = check(&CheckError::DistillUnknownArtifact {
        artifact: yunta_core::DistillArtifact {
            node: "plan".into(),
            id: yunta_core::ArtifactRefId::Name {
                name: "ghost.md".to_string(),
            },
        },
    });
    assert!(
        text.contains("ghost.md") && text.contains("distill"),
        "got: {text}"
    );
}

#[test]
fn a_node_declaring_the_same_kind_twice_is_refused() {
    let text = check(&CheckError::DuplicateArtifactKind {
        node: "plan".into(),
        kind: yunta_core::ArtifactKind::Tasks,
    });
    assert!(
        text.contains("at most one") && text.contains("identified by its kind"),
        "the refusal names the rule: {text}"
    );
}

#[test]
fn the_refusal_names_the_node_and_what_to_declare() {
    let text = check(&CheckError::Unset {
        node: "gate".into(),
        key: ConfigKey::Coverage,
        near: None,
    });
    assert!(
        text.contains("node `gate`")
            && text.contains("`coverage.cmd`")
            && text.contains("`coverage.threshold`"),
        "got: {text}"
    );
}

#[test]
fn a_mode_that_keeps_the_loop_and_drops_its_producer_is_refused() {
    let text = check(&CheckError::Unanswerable(Unanswerable::LoopWithoutTasks {
        node: "build".into(),
        mode: Some("quick".into()),
    }));
    assert!(text.contains("in mode `quick`"), "got: {text}");
}

#[test]
fn a_comparison_with_no_suite_to_measure_is_refused() {
    let text = check(&CheckError::BaselineWithoutSuite {
        sites: "node `no-regressions`".to_string(),
    });
    assert!(text.contains("`baseline.suite`"), "got: {text}");
}

#[test]
fn a_file_nothing_before_its_reader_can_write_is_refused() {
    let text = check(&CheckError::ContextFileUnreachable {
        node: "review".into(),
        path: "docs/review.md".to_string(),
        base: Some("36096d2c0a1b".to_string()),
        missing: yunta_core::caution::MissingContextFile::Nowhere,
    });
    assert!(
        text.contains("stops there every time") && text.contains("optional: true"),
        "the refusal says why and the way out: {text}"
    );
}

#[test]
fn a_reference_naming_a_kind_as_a_file_name_is_refused_where_it_is_written() {
    let text = check(&CheckError::ReservedArtifactName {
        place: yunta_core::workflow::Place::Read {
            node: "fix".into(),
            read: yunta_core::workflow::reads::ReadSite::Context,
        },
        name: "findings".to_string(),
    });
    assert!(
        text.contains("`fix`") && text.contains("kind: findings"),
        "the refusal names the place and the form that works: {text}"
    );
}

#[test]
fn a_composed_refusal_names_the_use_and_its_problems() {
    let refusal = CheckError::Composition(Composition::Fails {
        node: "sub".into(),
        name: "a".to_string(),
        problems: vec![CheckError::Unset {
            node: "gate".into(),
            key: yunta_core::ConfigKey::Coverage,
            near: None,
        }],
    });
    assert!(check(&refusal).starts_with("node `sub`: `use: a` fails check — node `gate`"));
}

#[test]
fn a_refused_answer_names_the_refusal_and_the_claim_it_answered() {
    let said = refused_answer(
        &Refusal::Withheld {
            withheld: withheld_proceed(),
        },
        &Claim::NeedsDecision {
            node: "gate".into(),
        },
        None,
    );
    assert_eq!(
        said,
        "the human surface's answer was refused: `proceed` is withheld: the plan cannot be \
         proven as it is written: the spec's test of `T001`, `test -f a.txt`, runs none of the \
         files the spec wrote, for: gate `gate` needs a decision"
    );
}

#[test]
fn an_artifact_view_that_cannot_be_written_names_the_object_it_lacks() {
    let hash = sha256_hex(b"report");
    let named = hash.to_string();
    assert_eq!(
        project_view(
            "report.md",
            &StoreError::Object(ObjectProblem::Missing { hash }),
        ),
        format!(
            "failed to write the `artifacts/` view of `report.md`: the run holds no object \
             `{named}` — an artifact names bytes that are not under `objects/`"
        )
    );
}

#[test]
fn a_decision_whose_documents_cannot_be_read_names_the_object_once() {
    let run = RunError::Object(ObjectProblem::Missing {
        hash: sha256_hex(b"plan"),
    });
    let said = resolve_gate(&ResolveGateError::Documents(Box::new(run)));
    assert!(
        said.starts_with(
            "the documents the decision shows cannot be read: the run holds no object `"
        ),
        "{said}"
    );
    assert_eq!(said.matches("the run holds no object").count(), 1, "{said}");
}

#[test]
fn an_input_the_command_was_not_given_or_gave_out_of_range_is_named_with_its_bound() {
    assert_eq!(
        inputs(&InputsError::Missing {
            name: "goal".into(),
        }),
        "input `goal` is required and has no default — pass `--input goal=...`"
    );
    assert_eq!(
        inputs(&InputsError::BelowMin {
            name: "count".into(),
            value: 0.0,
            min: 1.0,
        }),
        "input `count` must be >= 1, got 0"
    );
    assert_eq!(
        inputs(&InputsError::Unknown {
            names: vec!["x".into()],
            declared: vec!["goal".into()],
        }),
        "unknown input `x` — this workflow declares only: goal"
    );
}

#[test]
fn a_decision_the_run_cannot_take_says_why_and_what_to_choose_instead() {
    assert_eq!(
        resolve_gate(&ResolveGateError::NotPaused),
        "this run isn't parked at a pause — a live process may still be driving it, or it already \
         finished"
    );
    assert_eq!(
        resolve_gate(&ResolveGateError::UnknownOption {
            chosen: OptionId::from_static("retry"),
            declared: "proceed, abort".to_string(),
        }),
        "option `retry` isn't valid here — declared options: proceed, abort"
    );
    assert_eq!(
        resolve_gate(&ResolveGateError::Withheld {
            chosen: OptionId::from_static("proceed"),
            withheld: withheld_proceed(),
        }),
        "option `proceed` is withheld: the plan cannot be proven as it is written: the spec's \
         test of `T001`, `test -f a.txt`, runs none of the files the spec wrote"
    );
}

#[test]
fn a_run_that_cannot_be_closed_says_what_state_it_is_in() {
    assert_eq!(
        close(&CloseRunError::AlreadyClosed),
        "the run is already closed"
    );
    assert_eq!(
        close(&CloseRunError::Driven),
        "an engine is driving the run"
    );
    assert_eq!(
        close(&CloseRunError::Export("disk full".to_string())),
        "could not write the run's events.jsonl: disk full"
    );
}
