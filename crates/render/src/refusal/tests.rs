use super::check::line;
use super::{close, gate, hold_unclear, inputs, moving, refused_answer, Refused};
use crate::command::Reader;
use yunta_core::check::{CheckError, Composition, Unanswerable, UntakenRoute};
use yunta_core::events::{Claim, Refusal, WhyWithheld, Withheld};
use yunta_core::inputs::InputsProblem;
use yunta_core::refusal::{CloseRefusal, GateRefusal};
use yunta_core::shown::Flaw;
use yunta_core::{ArtifactSpec, ConfigKey, NonEmpty, OptionId, TaskId};

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
        line(&CheckError::DependsOnCycle {
            cycle: vec!["a".into(), "b".into(), "a".into()]
        }),
        "cycle in depends_on: a -> b -> a"
    );
    assert_eq!(
        line(&CheckError::UnknownRunner {
            node: "a".into(),
            runner: "planner".into(),
            near: None,
        }),
        "node `a` references runner `planner`, which `runners:` does not define"
    );
    assert_eq!(
        line(&CheckError::UnknownRunner {
            node: "a".into(),
            runner: "planer".into(),
            near: Some("planner".into()),
        }),
        "node `a` references runner `planer`, which `runners:` does not define; did you mean \
         `planner`?"
    );
}

#[test]
fn the_refusal_spells_the_split_and_how_to_read_the_answers() {
    let refusal = line(&CheckError::QuestionsAlongsideOtherArtifacts {
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
    let refusal = line(&CheckError::AnswersDeclaredAsProduced { node: "a".into() });
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
    let text = line(&clash);
    assert!(
        text.contains("`plan`") && text.contains("`plan-it`") && text.contains("tasks document"),
        "the refusal names both producers and the document they claim: {text}"
    );
}

#[test]
fn a_gate_that_routes_abort_somewhere_is_reported() {
    assert_eq!(
        line(&CheckError::GateRouteNeverTaken {
            node: "approve".into(),
            route: UntakenRoute::Abort,
        }),
        "gate `approve`: `on.abort` is never taken: choosing `abort` pauses the run; give the \
         option that sends the run back another id"
    );
}

#[test]
fn independent_nodes_with_overlapping_scope_error_under_parallel_fanout() {
    let text = line(&CheckError::OverlappingFanOutScope {
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
    let text = line(&CheckError::YuntaSchemaOutside {
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
    let text = line(&CheckError::DistillUnknownArtifact {
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
    let text = line(&CheckError::DuplicateArtifactKind {
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
    let text = line(&CheckError::Unset {
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
    let text = line(&CheckError::Unanswerable(Unanswerable::LoopWithoutTasks {
        node: "build".into(),
        mode: Some("quick".into()),
    }));
    assert!(text.contains("in mode `quick`"), "got: {text}");
}

#[test]
fn a_comparison_with_no_suite_to_measure_is_refused() {
    let text = line(&CheckError::BaselineWithoutSuite {
        sites: "node `no-regressions`".to_string(),
    });
    assert!(text.contains("`baseline.suite`"), "got: {text}");
}

#[test]
fn a_file_nothing_before_its_reader_can_write_is_refused() {
    let text = line(&CheckError::ContextFileUnreachable {
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
    let text = line(&CheckError::ReservedArtifactName {
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
    assert!(line(&refusal).starts_with("node `sub`: `use: a` fails check: node `gate`"));
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
fn an_input_the_command_was_not_given_or_gave_out_of_range_is_named_with_its_bound() {
    assert_eq!(
        inputs(&InputsProblem::Missing {
            name: "goal".into(),
        }),
        "input `goal` is required and has no default; pass `--input goal=...`"
    );
    assert_eq!(
        inputs(&InputsProblem::BelowMin {
            name: "count".into(),
            value: 0.0,
            min: 1.0,
        }),
        "input `count` must be >= 1, got 0"
    );
    assert_eq!(
        inputs(&InputsProblem::Unknown {
            names: vec!["x".into()],
            declared: vec!["goal".into()],
        }),
        "unknown input `x`: this workflow declares only: goal"
    );
}

/// The token cap is a decision with a menu, so this refusal is no longer about it.
#[test]
fn a_pause_with_no_menu_is_refused_without_naming_the_cap() {
    assert_eq!(
        gate(&GateRefusal::NothingToResolve),
        "this run isn't currently waiting on a decision `resolve_gate` can answer: it's paused \
         for a reason with no menu of options (a failed gate node, an external gate with no \
         forge)"
    );
}

#[test]
fn a_decision_the_run_cannot_take_says_why_and_what_to_choose_instead() {
    assert_eq!(
        gate(&GateRefusal::NotPaused),
        "this run isn't parked at a pause: a live process may still be driving it, or it already \
         finished"
    );
    assert_eq!(
        gate(&GateRefusal::UnknownOption {
            chosen: OptionId::from_static("retry"),
            declared: "proceed, abort".to_string(),
        }),
        "option `retry` isn't valid here; declared options: proceed, abort"
    );
    assert_eq!(
        gate(&GateRefusal::Withheld {
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
        close(&CloseRefusal::AlreadyClosed),
        "the run is already closed"
    );
    assert_eq!(close(&CloseRefusal::Driven), "an engine is driving the run");
}

/// The engine's pid in the sentences below.
fn holder() -> yunta_core::Pid {
    yunta_core::Pid::try_from(91930_u32).unwrap()
}

#[test]
fn a_moving_run_is_said_with_who_drives_it() {
    assert_eq!(
        moving("Q78GRR", holder()),
        "run Q78GRR is already moving: pid 91930 drives it"
    );
}

#[test]
fn a_hold_nobody_can_check_is_said_with_its_file_and_what_a_person_does() {
    let lock = std::path::Path::new("/runs/Q78GRR/scratch/engine.lock");
    assert_eq!(
        hold_unclear("Q78GRR", lock, Some(holder())),
        "run Q78GRR is held by pid 91930, a process this user cannot signal \
         (`/runs/Q78GRR/scratch/engine.lock`): if nothing drives this run, delete that file by \
         hand and resume it again"
    );
    assert_eq!(
        hold_unclear("Q78GRR", lock, None),
        "run Q78GRR has an engine lock with no readable owner \
         (`/runs/Q78GRR/scratch/engine.lock`): if nothing drives this run, delete that file by \
         hand and resume it again"
    );
}

/// A word that names a run of this repository, where the repository has none
/// it answers to, says so and points at the listing that shows what there is,
/// to a person and to an agent alike.
#[test]
fn a_word_no_run_answers_to_points_at_the_listing() {
    for reader in [Reader::Person, Reader::Agent] {
        assert_eq!(
            Refused::NoRunHere.said(reader),
            "no run in this repository; `yunta list --runs --all` lists every run"
        );
        assert_eq!(
            Refused::NoneNeedsYou.said(reader),
            "no run of this repository needs you; `yunta list --runs` lists them"
        );
    }
}
