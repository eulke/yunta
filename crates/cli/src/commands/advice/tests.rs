use chrono::TimeZone;
use yunta_core::events::{Escalation, Evidence, Failure, FailureMenu, GateOption, PauseReason};
use yunta_core::{NodeId, NonEmpty, OptionId};

use super::*;

#[test]
fn a_run_paused_on_a_reset_names_its_node_and_the_moment_it_resumes() {
    let at = chrono::Utc
        .with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
        .unwrap();
    let phase = RunPhase::Waiting {
        on: WaitingOn::Run {
            reason: PauseReason::AwaitingReset {
                node: NodeId::from_static("build"),
                at,
            },
        },
    };
    let (node, resumes) = reset_pause(&phase).expect("a run paused on a reset");
    assert_eq!((node.as_str(), resumes), ("build", at));
    assert!(reset_pause(&RunPhase::Running).is_none());
}

/// A decision on a failed node whose menu gives `stuck` as the reason no
/// remedy is offered, with nothing but abort on it.
fn decision_stuck_on(stuck: Stuck) -> GateWaitingPayload {
    let menu = FailureMenu {
        attempt: 1,
        failure: Failure::Message {
            outcome: "exit 1".to_string(),
        },
        remedies: Vec::new(),
        stuck: Some(stuck),
        reroute: None,
    };
    let abort = GateOption {
        id: OptionId::from_static("abort"),
        label: Some("Abort".to_string()),
        tradeoff: None,
        asks: None,
    };
    Escalation::new(
        "node `build` failed",
        Evidence::none(),
        NonEmpty::from((abort, Vec::new())),
    )
    .expect("the claim does not repeat its record")
    .with_failure(menu)
    .into_payload()
}

#[test]
fn a_failure_stuck_on_an_unchanged_tree_lists_the_resume_that_retries_once_it_changed() {
    let stuck = decision_stuck_on(Stuck::TreeUnchanged);
    assert_eq!(
        after(RunWord::NeedsYou, "run-1", Some(&stuck)),
        vec![
            (
                "yunta resume run-1".to_string(),
                "offers a retry once the run's tree changed",
            ),
            ("yunta close run-1".to_string(), "closes it for good"),
        ]
    );
    let denied = decision_stuck_on(Stuck::ProjectDenies);
    assert_eq!(
        after(RunWord::NeedsYou, "run-1", Some(&denied)),
        vec![("yunta close run-1".to_string(), "closes it for good")]
    );
}

#[test]
fn a_run_that_needs_a_person_without_a_decision_is_handed_back_once_settled() {
    assert_eq!(
        after(RunWord::NeedsYou, "run-1", None),
        vec![
            (
                "yunta resume run-1".to_string(),
                "hands it back once that is settled",
            ),
            ("yunta close run-1".to_string(), "closes it for good"),
        ]
    );
}

#[test]
fn a_run_waiting_on_a_reset_is_offered_the_reset_words() {
    assert_eq!(after(RunWord::Waiting, "run-1", None), after_reset("run-1"));
}

#[test]
fn spell_names_a_run_by_its_handle_in_the_command_that_takes_it() {
    let run = yunta_core::RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let steps = [
        (
            NextStep::Status {
                run: run.clone(),
                node: None,
            },
            "yunta status AAAAAA",
        ),
        (
            NextStep::Status {
                run: run.clone(),
                node: Some(NodeId::from_static("build")),
            },
            "yunta status AAAAAA --node build",
        ),
        (NextStep::Resume { run: run.clone() }, "yunta resume AAAAAA"),
        (NextStep::Cancel { run: run.clone() }, "yunta cancel AAAAAA"),
        (NextStep::Close { run: run.clone() }, "yunta close AAAAAA"),
        (
            NextStep::Receipt { run: run.clone() },
            "yunta receipt AAAAAA",
        ),
        (NextStep::Verify { run: run.clone() }, "yunta verify AAAAAA"),
        (NextStep::Gc, "yunta gc"),
        (
            NextStep::Check {
                workflow: "build-feature".to_string(),
            },
            "yunta check build-feature",
        ),
    ];
    for (step, words) in steps {
        assert_eq!(spell(&step), words);
    }
}

#[test]
fn spell_writes_a_decision_with_its_option_and_the_answer_it_asks_for() {
    let run = yunta_core::RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let retry = || Some(OptionId::from_static("retry"));
    let steps = [
        (
            NextStep::ResolveGate {
                run: run.clone(),
                option: None,
                text: false,
            },
            "yunta resolve-gate AAAAAA",
        ),
        (
            NextStep::ResolveGate {
                run: run.clone(),
                option: retry(),
                text: false,
            },
            "yunta resolve-gate AAAAAA retry",
        ),
        (
            NextStep::ResolveGate {
                run,
                option: retry(),
                text: true,
            },
            "yunta resolve-gate AAAAAA retry --text \"<answer>\"",
        ),
    ];
    for (step, words) in steps {
        assert_eq!(spell(&step), words);
    }
}

#[test]
fn an_agent_is_handed_the_whole_run_id_and_the_text_flag_for_an_option_that_asks() {
    let step = NextStep::ResolveGate {
        run: yunta_core::RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
        option: Some(OptionId::from_static("retry")),
        text: true,
    };
    assert_eq!(
        spell_for(&step, Reader::Agent),
        "yunta resolve-gate 01AAAAAAAAAAAAAAAAAAAAAAAA retry --text \"<answer>\""
    );
}

#[test]
fn the_test_spelling_matches_the_cli_for_every_step() {
    let run = yunta_core::RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let retry = || Some(OptionId::from_static("retry"));
    let test_spell = yunta_testkit_core::spell();
    let steps = [
        NextStep::Status {
            run: run.clone(),
            node: None,
        },
        NextStep::Status {
            run: run.clone(),
            node: Some(NodeId::from_static("build")),
        },
        NextStep::Resume { run: run.clone() },
        NextStep::Cancel { run: run.clone() },
        NextStep::Close { run: run.clone() },
        NextStep::Receipt { run: run.clone() },
        NextStep::Verify { run: run.clone() },
        NextStep::ResolveGate {
            run: run.clone(),
            option: None,
            text: false,
        },
        NextStep::ResolveGate {
            run: run.clone(),
            option: retry(),
            text: false,
        },
        NextStep::ResolveGate {
            run,
            option: retry(),
            text: true,
        },
        NextStep::Gc,
        NextStep::Check {
            workflow: "build-feature".to_string(),
        },
    ];
    for step in steps {
        assert_eq!(spell(&step), test_spell(&step), "{step:?}");
    }
}

#[test]
fn a_program_is_handed_the_whole_id_and_a_person_the_handle() {
    let whole = "01AAAAAAAAAAAAAAAAAAAAAAAA";
    let step = NextStep::Resume {
        run: yunta_core::RunId::from_static(whole),
    };
    assert_eq!(spell(&step), "yunta resume AAAAAA");
    assert_eq!(
        spell_for(&step, Reader::Agent),
        format!("yunta resume {whole}")
    );
}
