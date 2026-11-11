//! The field, through `State` alone: picking runs, closing them after one confirmation
//! naming each, what the field shows and asks while a person moves through it, and the
//! work it asks again while that work is under way.

mod support;

use std::path::PathBuf;

use surco::surface::{Markdown, Surface};
use yunta_api::{FieldReach, FieldReading};
use yunta_core::RunId;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Request, State};

use support::{on_the_field, stopped};

const FIRST: RunId = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
const SECOND: RunId = RunId::from_static("01BBBBBBBBBBBBBBBBBBBBBBBB");

fn pressed(state: &mut State, strokes: &[Stroke]) -> Vec<Request> {
    strokes
        .iter()
        .flat_map(|stroke| state.update(Input::Stroke(*stroke)))
        .collect()
}

/// Picking two runs and closing them asks one confirmation naming both, and closes
/// both once it is confirmed.
#[test]
fn closing_two_picked_runs_asks_once_naming_both_then_closes_both() {
    let mut state = on_the_field(vec![stopped(&FIRST), stopped(&SECOND)]);
    let asked = pressed(
        &mut state,
        &[
            Stroke::Insert(' '),
            Stroke::Insert('j'),
            Stroke::Insert(' '),
            Stroke::Insert('x'),
        ],
    );
    assert!(
        asked.is_empty(),
        "nothing is closed before it is confirmed: {asked:?}"
    );
    let look = Markdown.look();
    let overlay = state
        .screen(&look)
        .overlay
        .expect("a confirmation stands over the field");
    let asks = Markdown.draw(&overlay.doc);
    assert!(
        asks.contains("`AAAAAA`") && asks.contains("`BBBBBB`"),
        "{asks}"
    );

    let asked = pressed(&mut state, &[Stroke::Enter]);
    let [Request::Perform(_, Operation::Close { runs })] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!(runs, &[FIRST, SECOND]);
    assert!(
        state.screen(&look).overlay.is_none(),
        "the confirmation is answered"
    );
}

/// Going back from a confirmation closes nothing.
#[test]
fn going_back_from_a_confirmation_closes_nothing() {
    let mut state = on_the_field(vec![stopped(&FIRST)]);
    pressed(&mut state, &[Stroke::Insert('x')]);
    let asked = pressed(&mut state, &[Stroke::Decline]);
    assert!(asked.is_empty(), "{asked:?}");
    assert!(state.screen(&Markdown.look()).overlay.is_none());
}

/// The pointer starts on the first run listed, moves along the field and goes around.
#[test]
fn the_pointer_moves_along_the_field_and_goes_around() {
    let mut state = on_the_field(vec![stopped(&FIRST), stopped(&SECOND)]);
    let pointed = |state: &State| {
        let screen = Markdown.screen(&state.screen(&Markdown.look()));
        let row = screen
            .lines()
            .find(|line| line.contains('▸'))
            .unwrap_or_default();
        match (row.contains("AAAAAA"), row.contains("BBBBBB")) {
            (true, _) => "first",
            (_, true) => "second",
            _ => "none",
        }
    };
    assert_eq!(pointed(&state), "first");
    pressed(&mut state, &[Stroke::Insert('j')]);
    assert_eq!(pointed(&state), "second");
    pressed(&mut state, &[Stroke::Down]);
    assert_eq!(pointed(&state), "first");
    pressed(&mut state, &[Stroke::Insert('k')]);
    assert_eq!(pointed(&state), "second");
}

/// Resuming a run already being resumed asks nothing more: the work under way answers.
#[test]
fn resuming_a_run_already_resuming_asks_nothing_more() {
    let mut state = on_the_field(vec![stopped(&FIRST)]);
    let asked = pressed(&mut state, &[Stroke::Insert('r')]);
    let [Request::Perform(_, Operation::Resume { run })] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!(run, &FIRST);

    let asked = pressed(&mut state, &[Stroke::Insert('r')]);
    assert!(asked.is_empty(), "{asked:?}");
    assert_eq!(state.waiting_on().len(), 1, "{:?}", state.waiting_on());
}

/// Widening the reach and narrowing it again before the first reading is back lets that
/// reading go, and what it answers late is dropped.
#[test]
fn a_reading_asked_again_lets_go_the_one_under_way() {
    let mut state = on_the_field(vec![stopped(&FIRST)]);
    let asked = pressed(&mut state, &[Stroke::Insert('a')]);
    let [Request::Perform(
        wider,
        Operation::Field {
            reach: FieldReach::Machine,
        },
    )] = asked.as_slice()
    else {
        panic!("{asked:?}");
    };

    let asked = pressed(&mut state, &[Stroke::Insert('a')]);
    let [Request::LetGo(let_go), Request::Perform(
        _,
        Operation::Field {
            reach: FieldReach::Here,
        },
    )] = asked.as_slice()
    else {
        panic!("{asked:?}");
    };
    assert_eq!(let_go, wider);

    let late = Outcome::Field {
        reading: FieldReading {
            rows: Vec::new(),
            unreadable: Vec::new(),
            elsewhere: 0,
            here: false,
        },
        storage: PathBuf::from("/home/me/.yunta"),
    };
    assert!(state.update(Input::Outcome(*wider, late)).is_empty());
    let screen = Markdown.screen(&state.screen(&Markdown.look()));
    assert!(
        screen.contains("AAAAAA"),
        "the late reading is dropped: {screen}"
    );
}

/// A closing that fails partway says what failed, and the field is read again, so the
/// runs it closed before the failure are no longer listed.
#[test]
fn a_closing_that_fails_partway_says_so_and_reads_the_field_again() {
    let mut state = on_the_field(vec![stopped(&FIRST), stopped(&SECOND)]);
    pressed(
        &mut state,
        &[
            Stroke::Insert(' '),
            Stroke::Insert('j'),
            Stroke::Insert(' '),
            Stroke::Insert('x'),
        ],
    );
    let asked = pressed(&mut state, &[Stroke::Enter]);
    let [Request::Perform(closing, Operation::Close { .. })] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    let failed = "the disk refused the second run's close".to_string();
    let partway = Outcome::Closed {
        closed: vec![FIRST],
        refused: Vec::new(),
        failed: Some(failed.clone()),
    };
    let asked = state.update(Input::Outcome(*closing, partway));
    assert!(
        matches!(
            asked.as_slice(),
            [Request::Perform(_, Operation::Field { .. })]
        ),
        "{asked:?}"
    );
    let shown = Markdown.draw(&state.screen(&Markdown.look()).main.doc);
    assert!(shown.contains(&failed), "{shown}");
}
