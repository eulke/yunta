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
        (runs, "this repository's runs"),
        (NextStep::List, "the workflows you can run"),
        (doctor, "check adapters, git, forge, packs"),
        (NextStep::Gc { dry_run: false }, "remove orphaned checkouts"),
        (NextStep::Schemas, "the shape of every document yunta reads"),
    ];
    for (step, words) in palette {
        assert_eq!(does(&step), words, "{}", line(&step));
    }
}

/// Each act the field's palette offers, in the design's words: following the run pointed
/// at, going to what needs the person, and the field again, with its closed runs or across
/// the machine.
#[test]
fn the_palette_words_each_act_it_offers_over_the_field() {
    let palette = [
        (
            NextStep::Follow {
                run: RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
            },
            "follow it as it moves",
        ),
        (NextStep::Needs, "go to what needs you"),
        (
            NextStep::ListRuns {
                reach: Reach::WithClosed,
            },
            "this repository's runs, the closed too",
        ),
        (
            NextStep::ListRuns {
                reach: Reach::Machine,
            },
            "the runs on this machine",
        ),
        (
            NextStep::ListRuns {
                reach: Reach::MachineWithClosed,
            },
            "every run on this machine, the closed too",
        ),
    ];
    for (step, words) in palette {
        assert_eq!(does(&step), words, "{}", line(&step));
    }
}

/// An effect the palette words as an act says the palette's words; one it does not says its
/// own, in the same register: an order, or what it shows.
#[test]
fn each_effect_says_what_its_action_does_to_the_run() {
    let said = [
        (Effect::ContinuesFromLog, "continue it from its log"),
        (
            Effect::ContinuesWhereStopped,
            "continue it where it stopped",
        ),
        (Effect::WaitsForReset, "wait for the reset, then drive on"),
        (
            Effect::HandsBackOnceSettled,
            "hand it back once that is settled",
        ),
        (
            Effect::RetriesOnceTreeChanged,
            "retry it once the run's tree changed",
        ),
        (
            Effect::ReadsPlan,
            "read the plan it waits on, and answer it there",
        ),
        (Effect::ShowsWhatItWaitsOn, "what it is waiting on"),
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
        (NextStep::Init { interactive: false }, "yunta init"),
        (NextStep::Init { interactive: true }, "yunta init -i"),
    ];
    for (step, written) in lines {
        assert_eq!(line(&step), written);
    }
}

/// Each step no palette offers, by the line a person types for it, beside what it does in the
/// register the palette's are in: an order for one that changes something, what it shows for
/// one that shows something.
const SAID_AS_THE_PALETTE_SAYS: [(&str, &str); 26] = [
    ("yunta status AAAAAA", "where the run stands"),
    ("yunta status AAAAAA --full", "the run's whole story"),
    (
        "yunta status AAAAAA --dig",
        "its story, with every trace it keeps",
    ),
    (
        "yunta status AAAAAA --node build",
        "one node of the run, whole",
    ),
    (
        "yunta review AAAAAA",
        "read the plan it waits on, and answer it there",
    ),
    (
        "yunta resolve-gate AAAAAA <option>",
        "answer the decision it waits on",
    ),
    (
        "yunta resolve-gate AAAAAA retry",
        "answer the decision it waits on",
    ),
    (
        "yunta stats --workflow lint-fix",
        "what the workflow's past runs cost",
    ),
    (
        "yunta check <workflow>",
        "check the workflow, each broken rule named",
    ),
    ("yunta graph <workflow>", "draw the workflow's graph"),
    (
        "yunta schema workflow --json",
        "the JSON Schema an editor validates against",
    ),
    ("yunta schema tasks", "a document's shape"),
    (
        "yunta doctor --session",
        "open a session on each agent binding",
    ),
    (
        "yunta doctor --full",
        "check this machine, every check unfolded",
    ),
    ("yunta test", "run the project's workflow test cases"),
    ("yunta init", "prepare the repository for yunta"),
    ("yunta new review", "write a new workflow from a skeleton"),
    ("yunta pack add <source>", "install a pack"),
    (
        "yunta pack update acme/lint <ref>",
        "update a pack to another reference",
    ),
    ("yunta pack remove acme/lint", "remove a pack"),
    (
        "yunta pack new acme/lint",
        "write the skeleton a new pack starts from",
    ),
    (
        "yunta pack list",
        "the installed packs, each checked against the lock",
    ),
    (
        "yunta pack audit acme/lint",
        "audit everything a pack holds and asks for",
    ),
    (
        "yunta gc --dry-run",
        "what reclaiming would remove; nothing is removed",
    ),
    (
        "yunta --help",
        "what yunta does, and every command it takes",
    ),
    ("git show 4f6c0d3", "the commit the run's work is held in"),
];

/// Each step no palette offers says what it does as the palette would: the sample of it a
/// person types as a line of the table says that line's words.
#[test]
fn a_step_no_palette_offers_is_an_order_or_what_it_shows() {
    let steps = every_step();
    for (typed, words) in SAID_AS_THE_PALETTE_SAYS {
        let step = steps
            .iter()
            .find(|step| line(step) == typed)
            .unwrap_or_else(|| panic!("no sample of a step is typed `{typed}`"));
        assert_eq!(does(step), words, "{typed}");
    }
}

/// The verbs the words beside a command never open with: each says what the command does in
/// the third person, where the palette orders the act or names what it shows.
const THIRD_PERSON: [&str; 20] = [
    "says", "tells", "shows", "reads", "answers", "draws", "checks", "prints", "opens", "runs",
    "prepares", "writes", "installs", "updates", "removes", "lists", "audits", "waits", "hands",
    "offers",
];

/// How many effects [`effect_number`] numbers: one per variant of `Effect`.
const EFFECTS: usize = 11;

/// Each effect by a number of its own. An effect added to `Effect` does not build here until
/// it has one, and [`every_effect`] then asks for it.
fn effect_number(effect: Effect) -> usize {
    match effect {
        Effect::ContinuesFromLog => 0,
        Effect::ContinuesWhereStopped => 1,
        Effect::WaitsForReset => 2,
        Effect::HandsBackOnceSettled => 3,
        Effect::RetriesOnceTreeChanged => 4,
        Effect::ReadsPlan => 5,
        Effect::ShowsWhatItWaitsOn => 6,
        Effect::Stops => 7,
        Effect::Closes => 8,
        Effect::Certifies => 9,
        Effect::Verifies => 10,
    }
}

/// Every effect an action can have, each number [`effect_number`] gives held once.
fn every_effect() -> [Effect; EFFECTS] {
    let every = [
        Effect::ContinuesFromLog,
        Effect::ContinuesWhereStopped,
        Effect::WaitsForReset,
        Effect::HandsBackOnceSettled,
        Effect::RetriesOnceTreeChanged,
        Effect::ReadsPlan,
        Effect::ShowsWhatItWaitsOn,
        Effect::Stops,
        Effect::Closes,
        Effect::Certifies,
        Effect::Verifies,
    ];
    let numbered: BTreeSet<usize> = every.into_iter().map(effect_number).collect();
    assert_eq!(numbered, (0..EFFECTS).collect(), "an effect is left out");
    every
}

/// Nothing beside a command is said in the third person, what a step does nor what an action
/// does to its run, so one block of next steps never mixes moods. The samples hold a step of
/// every kind and every effect, so a kind or an effect added later is held here too.
#[test]
fn nothing_beside_a_command_is_said_in_the_third_person() {
    let steps = every_step()
        .into_iter()
        .map(|step| (line(&step), does(&step)));
    let actions = every_effect().map(|effect| (format!("{effect:?}"), super::effect(effect)));
    for (said_of, words) in steps.chain(actions) {
        let first = words.split_whitespace().next().unwrap_or_default();
        assert!(
            !THIRD_PERSON.contains(&first),
            "`{said_of}` says `{words}`, in the third person"
        );
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
