//! What help lists: the sentence it opens on, the keys every screen answers to and a
//! run's own in the design's words, and over a prompt standing in place on a run's screen
//! the prompt's keys in its words, no row of the run's for those keys, and none a line typed
//! there swallows.

use surco::components::{KeyList, Legend};
use surco::doc::Block;
use surco::glyphs::Glyphs;
use yunta_core::step::{NextStep, Reach};

use crate::prompt::card::{self, ChosenWith};
use crate::prompt::{desk, round, PromptAct};
use crate::screen::{
    help, help_over_a_run, help_over_the_field, Along, GoingDown, Leaving, PromptKeys, StageKeys,
};

/// The keys `doc` lists, the sentence above them first, each key beside what it does.
fn listed(doc: &surco::doc::Doc) -> (Option<String>, Vec<(String, String)>) {
    let Some(Block::KeyList(KeyList { said, keys })) = doc.blocks().first() else {
        panic!("help is a list of keys");
    };
    let keys = keys.0.iter().map(|binding| {
        let key = binding.keys.spelled(Glyphs::Unicode).to_string();
        (key, binding.does.clone())
    });
    (said.clone(), keys.collect())
}

/// The keys of a page whose legend is `legend`, taking nothing beyond it nor anything every
/// stage answers to but the palette and Ctrl-C.
fn bare(legend: &Legend) -> StageKeys<'_> {
    StageKeys {
        legend,
        beyond: Legend::new(),
        new_run: false,
        attends: false,
        scrolls: false,
        down: None,
    }
}

/// Help opens on the sentence that says it holds every key there is.
#[test]
fn helps_first_row_is_the_tagline() {
    let legend = Legend::new().key("j k", "move");
    let (said, _) = listed(&help(bare(&legend)));
    assert_eq!(
        said.as_deref(),
        Some("You never have to remember more than this.")
    );
}

/// Help over a page says the keys every stage answers to wherever each acts there, its
/// legend's keys and those it takes beyond them, each key once, where it is first named: a
/// page over the field that scrolls, where a run can be asked for, its legend naming a new
/// run as help does first.
#[test]
fn help_over_a_page_says_each_key_it_takes_once() {
    let legend = Legend::new().key("esc", "back").key("n", "new run");
    let keys = StageKeys {
        beyond: Legend::new().key("u", "what collecting would reclaim"),
        new_run: true,
        attends: true,
        scrolls: true,
        down: Some(GoingDown::ToTheField),
        ..bare(&legend)
    };
    let (_, rows) = listed(&help(keys));
    let rows: Vec<String> = rows
        .into_iter()
        .map(|(key, does)| format!("{key} {does}"))
        .collect();
    assert_eq!(
        rows,
        [
            "/ anything: only what can be done here, with its command",
            "n start a new run, wherever no prompt stands",
            "esc back",
            "u what collecting would reclaim",
            "↑↓ scroll",
            "pgup pgdn by the page",
            "a go to what needs you",
            "q go to the field",
            "ctrl-c leave: every run keeps going without this window",
        ]
    );
    let bottom = StageKeys {
        down: Some(GoingDown::ToWhereItOpened),
        ..bare(&legend)
    };
    let (_, rows) = listed(&help(bottom));
    let q = ("q".to_string(), "go back to where yunta opened".to_string());
    assert!(rows.contains(&q), "{rows:#?}");
}

/// Help over a run lists the keys every screen answers to in the design's words, the
/// run's own, then the acts its rows do not say, as the target `help` lists them: what it
/// cost, its graph and a new run have rows of their own.
#[test]
fn help_over_a_run_lists_the_designs_rows() {
    let acts = [
        ('c', "stop it and everything under it"),
        ('s', "stats"),
        ('g', "graph"),
        ('o', "the whole page, in a browser"),
        ('n', "new run"),
    ];
    let (said, keys) = listed(&help_over_a_run(
        Along::Nodes,
        &acts,
        None,
        Leaving::KeepsGoing,
    ));
    assert!(said.is_some());
    let keys: Vec<String> = keys
        .into_iter()
        .map(|(key, does)| format!("{key} {does}"))
        .collect();
    let rows = [
        "/ anything: only what can be done here, with its command",
        "n start a new run, wherever no prompt stands",
        "j k move; the footer always says what they move",
        "pgup pgdn scroll the pane a page at a time",
        "enter open, answer, confirm",
        "esc one step back; what it leaves stays waiting",
        "a go to what needs you",
        "f follow what the run is doing",
        "s g what it cost, its graph",
        "c stop it and everything under it",
        "o the whole page, in a browser",
        "q go to the field; it keeps going",
        "ctrl-c leave: every run keeps going without this window",
    ];
    assert_eq!(keys, rows);
}

/// Help over the field lists the keys every screen answers to, then the field's own, those
/// its legend leaves out among them, in the design's words, as the target `field-help` lists
/// them; `d` and `e` say what they do from where the field stands, the closed shown or the
/// machine's runs listed; outside a repository no row names `e`.
#[test]
fn help_over_the_field_lists_the_designs_rows() {
    let rows = |closed: bool, farther: Option<Reach>| -> Vec<String> {
        let farther = farther.map(|reach| NextStep::ListRuns { reach });
        let (said, keys) = listed(&help_over_the_field(closed, true, farther.as_ref()));
        assert!(said.is_some());
        keys.into_iter()
            .map(|(key, does)| format!("{key} {does}"))
            .collect()
    };
    let designed = [
        "/ anything: only what can be done here, with its command",
        "n start a new run, wherever no prompt stands",
        "j k move; the footer always says what they move",
        "enter open, answer, confirm",
        "space mark it",
        "x close the marked, or the pointed, once confirmed",
        "r resume it",
        "c stop it and everything under it",
        "s what it cost",
        "a go to what needs you",
        "d the closed too",
        "e the runs on this machine",
        "w the workflows you can run",
        "m check adapters, git, forge, packs",
        "q leave: every run keeps going without this window",
        "ctrl-c leave: every run keeps going without this window",
    ];
    assert_eq!(rows(false, Some(Reach::Machine)), designed);
    let turned = rows(true, Some(Reach::WithClosed));
    for said in [
        "d hides the closed",
        "e this repository's runs, the closed too",
    ] {
        assert!(turned.contains(&said.to_string()), "{}", turned.join("\n"));
    }
    let outside = rows(false, None);
    let names_e = outside.iter().any(|row| row.starts_with("e "));
    assert!(!names_e, "{outside:#?}");
}

/// Each row help over a run lists under `prompt`, standing in place: its keys, then what
/// each does.
fn rows_under(prompt: PromptKeys<'_>) -> Vec<(String, String)> {
    let acts = [
        ('c', "stop it and everything under it"),
        ('o', "the whole page"),
    ];
    listed(&help_over_a_run(
        Along::Nodes,
        &acts,
        Some(prompt),
        Leaving::Waits,
    ))
    .1
}

/// Help over a run whose card stands in place says every key the card takes in the card's
/// words, first, those its legend names and those it leaves to help, and the run's row for
/// such a key stands nowhere: `n` adds a note, and no row says it starts a run; `tab`,
/// `shift-tab` and the numbers choose. The run's keys the card leaves stand after it.
#[test]
fn help_over_a_card_in_place_says_the_cards_keys_in_its_words() {
    let card = card::choosing_legend(ChosenWith::Letters, true, PromptAct::LeaveItWaiting.said());
    let Legend(keys) = card.clone();
    let takes = Legend(keys.into_iter().chain(card::left_to_help(2).0).collect());
    let rows = rows_under(PromptKeys {
        legend: &takes,
        typed: false,
    });
    let said = |key: &str| -> Vec<&str> {
        rows.iter()
            .filter(|(keys, _)| keys.split_whitespace().any(|named| named == key))
            .map(|(_, does)| does.as_str())
            .collect()
    };
    assert_eq!(said("n"), [PromptAct::AddANote.said()], "{rows:#?}");
    assert_eq!(said("j"), [PromptAct::Choose.said()], "{rows:#?}");
    assert_eq!(said("enter"), [PromptAct::Answer.said()], "{rows:#?}");
    assert_eq!(said("esc"), [PromptAct::LeaveItWaiting.said()], "{rows:#?}");
    for key in ["tab", "shift-tab", "1", "2"] {
        assert_eq!(said(key), [PromptAct::Choose.said()], "{key}: {rows:#?}");
    }
    let first: Vec<&str> = rows
        .iter()
        .take(takes.0.len())
        .map(|(keys, _)| keys.as_str())
        .collect();
    assert_eq!(
        first,
        ["j k", "enter", "n", "r", "esc", "tab shift-tab", "1 2"],
        "the card's keys first"
    );
    let stands = |kept: &str| rows.iter().any(|(keys, _)| keys == kept);
    for kept in ["/", "a", "f", "s g", "c", "o", "q", "ctrl-c"] {
        assert!(stands(kept), "{kept:?} in {rows:#?}");
    }
}

/// Help over a run whose plan's desk stands in place says every key the desk takes in its
/// words: its legend's, its numbers, the next place, the arrows, its proof and `esc` once;
/// and those it leaves to help, `j k` to the next place as `tab`, the place before and a page
/// at a time. No row of the run's says `j k` move along the nodes, nor `pgup pgdn` scroll the
/// pane.
#[test]
fn help_over_the_desk_in_place_says_every_key_the_desk_takes_in_its_words() {
    let legend = desk::legend(3, true, false, PromptAct::LeaveItWaiting.said());
    let Legend(keys) = legend;
    let takes = Legend(keys.into_iter().chain(desk::left_to_help().0).collect());
    let rows: Vec<String> = rows_under(PromptKeys {
        legend: &takes,
        typed: false,
    })
    .into_iter()
    .map(|(key, does)| format!("{key} {does}"))
    .collect();
    for row in [
        "1 2 3 answer",
        "tab next place",
        "↑↓ scroll",
        "p proof",
        "esc leave it waiting",
        "j k next place",
        "shift-tab place before",
        "pgup pgdn by the page",
    ] {
        assert!(rows.iter().any(|said| said == row), "{row:?} in {rows:#?}");
    }
    for key in ["j k ", "pgup pgdn ", "esc "] {
        let said = rows.iter().filter(|said| said.starts_with(key)).count();
        assert_eq!(said, 1, "{key:?} once in {rows:#?}");
    }
}

/// Over a prompt a line is typed on, a round or a typed line on a card or the desk, every
/// printable key types: help says the prompt's keys and Ctrl-C alone, no run's key the line
/// swallows.
#[test]
fn help_over_a_typed_line_offers_no_run_key_the_line_swallows() {
    let round = round::legend(true, true, PromptAct::LeaveItForLater.said());
    let typed_line = card::saying_legend(true);
    for (what, legend) in [("a round", &round), ("a typed line", &typed_line)] {
        let rows = rows_under(PromptKeys {
            legend,
            typed: true,
        });
        let keys: Vec<&str> = rows.iter().map(|(keys, _)| keys.as_str()).collect();
        let own: Vec<String> = legend
            .0
            .iter()
            .map(|binding| binding.keys.spelled(Glyphs::Unicode).to_string())
            .collect();
        let expected: Vec<&str> = own.iter().map(String::as_str).chain(["ctrl-c"]).collect();
        assert_eq!(keys, expected, "{what}: {rows:#?}");
    }
}
