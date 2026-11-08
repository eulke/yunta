use surco::doc::Depth;
use yunta_core::step::{Arg, Choice, NextStep};
use yunta_core::{NodeId, OptionId, RunId};

use super::*;

#[test]
fn a_line_names_a_run_by_its_handle_in_the_command_that_takes_it() {
    let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let steps = [
        (
            NextStep::Status {
                run: run.clone(),
                node: None,
                depth: Depth::Glance,
            },
            "yunta status AAAAAA",
        ),
        (
            NextStep::Status {
                run: run.clone(),
                node: Some(NodeId::from_static("build")),
                depth: Depth::Glance,
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
                workflow: Arg::Given("build-feature".to_string()),
            },
            "yunta check build-feature",
        ),
    ];
    for (step, words) in steps {
        assert_eq!(line(&step), words);
    }
}

#[test]
fn a_line_writes_a_decision_with_its_option_and_the_answer_it_asks_for() {
    let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let retry = |words| Choice::Picked {
        option: OptionId::from_static("retry"),
        words,
    };
    let steps = [
        (
            NextStep::ResolveGate {
                run: run.clone(),
                choice: Choice::Asked,
            },
            "yunta resolve-gate AAAAAA",
        ),
        (
            NextStep::ResolveGate {
                run: run.clone(),
                choice: retry(false),
            },
            "yunta resolve-gate AAAAAA retry",
        ),
        (
            NextStep::ResolveGate {
                run,
                choice: retry(true),
            },
            "yunta resolve-gate AAAAAA retry --text \"<answer>\"",
        ),
    ];
    for (step, words) in steps {
        assert_eq!(line(&step), words);
    }
}

#[test]
fn an_agent_is_handed_the_whole_run_id_and_the_text_flag_for_an_option_that_asks() {
    let step = NextStep::ResolveGate {
        run: RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
        choice: Choice::Picked {
            option: OptionId::from_static("retry"),
            words: true,
        },
    };
    assert_eq!(
        line_for(&step, Reader::Agent),
        "yunta resolve-gate 01AAAAAAAAAAAAAAAAAAAAAAAA retry --text \"<answer>\""
    );
}

#[test]
fn a_program_is_handed_the_whole_id_and_a_person_the_handle() {
    let whole = "01AAAAAAAAAAAAAAAAAAAAAAAA";
    let step = NextStep::Resume {
        run: RunId::from_static(whole),
    };
    assert_eq!(line(&step), "yunta resume AAAAAA");
    assert_eq!(
        line_for(&step, Reader::Agent),
        format!("yunta resume {whole}")
    );
}
