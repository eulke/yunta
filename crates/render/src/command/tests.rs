use std::collections::{BTreeMap, BTreeSet};

use surco::doc::Depth;
use yunta_core::step::samples::every_step;
use yunta_core::step::{Arg, Choice, NextStep, Reach};
use yunta_core::{NodeId, OptionId, PackRef, RunId, WorkflowName, WorkflowShape};
use yunta_replay::frame::{Action, Effect};

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
        (NextStep::Gc { dry_run: false }, "yunta gc"),
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

/// Each act a run's palette offers on the run, in the palette's words: what it does, as
/// the person would ask for it.
#[test]
fn the_palette_words_each_act_on_a_run() {
    let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let answer = NextStep::ResolveGate {
        run: run.clone(),
        choice: Choice::Asked,
    };
    let palette = [
        (answer, "answer what it asks"),
        (NextStep::Cancel { run: run.clone() }, STOP),
        (NextStep::Resume { run: run.clone() }, CONTINUE),
        (NextStep::Close { run: run.clone() }, CLOSE),
        (NextStep::Receipt { run: run.clone() }, WRITE_THE_RECEIPT),
        (NextStep::Verify { run: run.clone() }, VERIFY),
        (NextStep::RunGraph { run: run.clone() }, "draw its graph"),
        (NextStep::Open { run }, "the whole page, in a browser"),
    ];
    for (step, words) in palette {
        assert_eq!(does(&step), words, "{}", line(&step));
    }
    assert_eq!(STOP, "stop it and everything under it");
    assert_eq!(CONTINUE, "continue it from its log");
    assert_eq!(CLOSE, "close it for good");
    assert_eq!(WRITE_THE_RECEIPT, "write the receipt");
    assert_eq!(VERIFY, "verify its evidence");
}

/// Each act the palette offers from anywhere, in the palette's words.
#[test]
fn the_palette_words_each_act_it_offers_from_anywhere() {
    let stats = NextStep::Stats {
        run: RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
    };
    let doctor = NextStep::Doctor {
        sessions: false,
        full: false,
    };
    let runs = NextStep::ListRuns {
        reach: Reach::Repository,
    };
    let palette = [
        (stats, "what it cost, and where its time went"),
        (
            NextStep::Run {
                workflow: Arg::Blank,
            },
            "start a new run",
        ),
        (runs, "every run of this repository"),
        (NextStep::List, "the workflows you can run"),
        (doctor, "check adapters, git, forge, packs"),
        (NextStep::Gc { dry_run: false }, "remove orphaned checkouts"),
        (NextStep::Schemas, "the shape of every document yunta reads"),
    ];
    for (step, words) in palette {
        assert_eq!(does(&step), words, "{}", line(&step));
    }
}

/// An effect the palette words as an act says the palette's words; one it does not
/// keeps its own.
#[test]
fn each_effect_says_what_its_action_does_to_the_run() {
    let said = [
        (Effect::ContinuesFromLog, "continue it from its log"),
        (
            Effect::ContinuesWhereStopped,
            "continues it where it stopped",
        ),
        (Effect::WaitsForReset, "waits for the reset, then drives on"),
        (
            Effect::HandsBackOnceSettled,
            "hands it back once that is settled",
        ),
        (
            Effect::RetriesOnceTreeChanged,
            "offers a retry once the run's tree changed",
        ),
        (Effect::ReadsPlan, "reads its plan, and answers it there"),
        (Effect::ShowsWhatItWaitsOn, "shows what it is waiting on"),
        (Effect::Stops, "stop it and everything under it"),
        (Effect::Closes, "close it for good"),
        (Effect::Certifies, "write the receipt"),
        (Effect::Verifies, "verify its evidence"),
    ];
    for (effect, words) in said {
        assert_eq!(super::effect(effect), words);
    }
}

#[test]
fn an_action_is_said_as_its_line_and_what_it_does() {
    let action = Action {
        step: NextStep::Close {
            run: RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
        },
        effect: Effect::Closes,
    };
    assert_eq!(
        said(&action, Reader::Person),
        ("yunta close AAAAAA".to_string(), "close it for good")
    );
}

/// The commands a person types that no run's action names: what a run and a
/// workflow cost, their graphs, a new workflow, the packs listed and audited, and
/// a reclaim that only says what it would do.
#[test]
fn every_command_a_person_types_has_its_line() {
    let run = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
    let pack = "acme/lint".parse::<PackRef>().expect("a pack reference");
    let lines = [
        (NextStep::Stats { run: run.clone() }, "yunta stats AAAAAA"),
        (
            NextStep::History {
                workflow: WorkflowName::from("review"),
            },
            "yunta stats --workflow review",
        ),
        (
            NextStep::Graph {
                workflow: Arg::Given("review".to_string()),
            },
            "yunta graph review",
        ),
        (
            NextStep::Graph {
                workflow: Arg::Blank,
            },
            "yunta graph <workflow>",
        ),
        (NextStep::RunGraph { run }, "yunta graph --run AAAAAA"),
        (
            NextStep::NewWorkflow {
                name: Arg::Given("review".to_string()),
                shape: None,
            },
            "yunta new review",
        ),
        (
            NextStep::NewWorkflow {
                name: Arg::Blank,
                shape: Some(WorkflowShape::Tasks),
            },
            "yunta new <name> --shape tasks",
        ),
        (NextStep::PackList, "yunta pack list"),
        (NextStep::PackAudit { pack }, "yunta pack audit acme/lint"),
        (NextStep::Gc { dry_run: true }, "yunta gc --dry-run"),
    ];
    for (step, written) in lines {
        assert_eq!(line(&step), written);
    }
}

/// Every step says what it does, beside its line on a palette and in help, and two
/// kinds of step never say the same: a reader picks a step by what it does.
#[test]
fn every_step_says_what_it_does() {
    let mut kinds_by_words: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for step in &every_step() {
        let words = does(step);
        assert!(!words.trim().is_empty(), "`{}` says nothing", line(step));
        let kind = format!("{:?}", std::mem::discriminant(step));
        kinds_by_words.entry(words).or_default().insert(kind);
    }
    for (words, kinds) in &kinds_by_words {
        assert_eq!(
            kinds.len(),
            1,
            "`{words}` is said of {} kinds of step",
            kinds.len()
        );
    }
    let follow = NextStep::Follow {
        run: RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
    };
    assert_eq!(does(&follow), "follow it as it moves");
}
