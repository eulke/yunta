//! The field, through `State` alone: picking runs, closing them after one confirmation
//! naming each, what the field shows and asks while a person moves through it, and the
//! field drawn from what is followed: its runs moving to their groups with no stroke, its
//! ages at the clock, its reach followed, a look that failed said over it.

mod support;

use chrono::TimeDelta;
use surco::components::Tally;
use surco::surface::{Markdown, Surface};
use surco::words::Standing;
use tokio::sync::mpsc;
use yunta_api::{FieldReach, Followed, Stop};
use yunta_core::RunId;
use yunta_testkit::workflows::GATED;
use yunta_testkit::Bench;
use yunta_testkit_core::fixed_now;
use yunta_testkit_ui::driving::drive;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Request, State};

use support::{on_the_field, performer_in, stopped_beside};

const FIRST: RunId = RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
const SECOND: RunId = RunId::from_static("01BBBBBBBBBBBBBBBBBBBBBBBB");
/// A run whose workflow parks on a gate once it is woken with nothing stopping it.
const GATE: RunId = RunId::from_static("01CCCCCCCCCCCCCCCCCCCCCCCC");

fn pressed(state: &mut State, strokes: &[Stroke]) -> Vec<Request> {
    strokes
        .iter()
        .flat_map(|stroke| state.update(Input::Stroke(*stroke)))
        .collect()
}

/// The row the pointer stands on, as a person reads the field.
fn pointed_row(state: &State) -> String {
    let screen = Markdown.screen(&state.screen(&Markdown.look()));
    let row = screen.lines().find(|line| line.contains('▸'));
    row.unwrap_or_default().to_string()
}

/// `bench`'s run woken again with nothing stopping it: a gated run parks on its gate.
async fn woken(bench: Bench) {
    bench.with_cancel(Stop::new().work().clone()).wake().await;
}

/// Picking two runs and closing them asks one confirmation naming both, and closes
/// both once it is confirmed.
#[tokio::test]
async fn closing_two_picked_runs_asks_once_naming_both_then_closes_both() {
    let (mut state, _) = on_the_field(&[&FIRST, &SECOND]).await;
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
#[tokio::test]
async fn going_back_from_a_confirmation_closes_nothing() {
    let (mut state, _) = on_the_field(&[&FIRST]).await;
    pressed(&mut state, &[Stroke::Insert('x')]);
    let asked = pressed(&mut state, &[Stroke::Decline]);
    assert!(asked.is_empty(), "{asked:?}");
    assert!(state.screen(&Markdown.look()).overlay.is_none());
}

/// The pointer starts on the first run listed, moves along the field and goes around.
#[tokio::test]
async fn the_pointer_moves_along_the_field_and_goes_around() {
    let (mut state, _) = on_the_field(&[&FIRST, &SECOND]).await;
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
#[tokio::test]
async fn resuming_a_run_already_resuming_asks_nothing_more() {
    let (mut state, _) = on_the_field(&[&FIRST]).await;
    let asked = pressed(&mut state, &[Stroke::Insert('r')]);
    let [Request::Perform(_, Operation::Resume { run })] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!(run, &FIRST);

    let asked = pressed(&mut state, &[Stroke::Insert('r')]);
    assert!(asked.is_empty(), "{asked:?}");
    assert_eq!(state.waiting_on().len(), 1, "{:?}", state.waiting_on());
}

/// `a` follows the machine's field in the field's place, with no reading of it asked, and
/// the pointer stays on the run it stood on once that field is read.
#[tokio::test]
async fn widening_the_reach_follows_the_machines_field_and_keeps_the_pointer() {
    let (mut state, world) = on_the_field(&[&FIRST, &SECOND]).await;
    pressed(&mut state, &[Stroke::Insert('j')]);
    let asked = pressed(&mut state, &[Stroke::Insert('a')]);
    let machine = Followed {
        field: Some(FieldReach::Machine),
        run: None,
    };
    assert_eq!(asked, [Request::Follow(machine)]);

    let (state, _) = drive(state, asked, &performer_in(&world), &[]).await;
    let field = Markdown.screen(&state.screen(&Markdown.look()));
    assert!(field.contains("runs on this machine"), "{field}");
    assert!(pointed_row(&state).contains(SECOND.handle()), "{field}");
}

/// A closing that fails partway says what failed and asks no reading: the field moves as
/// its runs do, by what is followed.
#[tokio::test]
async fn a_closing_that_fails_partway_says_so_and_asks_no_reading() {
    let (mut state, _) = on_the_field(&[&FIRST, &SECOND]).await;
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
    assert!(asked.is_empty(), "{asked:?}");
    let shown = Markdown.draw(&state.screen(&Markdown.look()).main.doc);
    assert!(shown.contains(&failed), "{shown}");
}

/// A run of the field its bench parks on its gate while the field stands moves under needs
/// you once its news is handed, with no stroke: the tally and the attention row count it,
/// and the pointer stays on it.
#[tokio::test]
async fn a_run_parked_while_the_field_stands_moves_under_needs_you() {
    let (state, world) = on_the_field(&[&FIRST, &SECOND]).await;
    let gate = stopped_beside(&world, &GATE, GATED).await;
    let performer = performer_in(&world);
    let onto_it = [Stroke::Insert('j'), Stroke::Insert('j')];
    let (state, _) = drive(state, Vec::new(), &performer, &onto_it).await;
    let stopped = pointed_row(&state);
    assert!(stopped.contains(GATE.handle()), "{stopped}");

    woken(gate).await;
    let (state, _) = drive(state, Vec::new(), &performer, &[]).await;
    let screen = state.screen(&Markdown.look());
    let drawn = Markdown.screen(&screen);
    let needs_you = drawn
        .split("needs you, oldest first")
        .nth(1)
        .and_then(|below| below.split(Standing::Stopped.word()).next())
        .unwrap_or_default();
    assert!(needs_you.contains(GATE.handle()), "{drawn}");
    let counted: &[_] = match &screen.tally {
        Tally::Counted(counts) => counts,
        Tally::Uncounted => &[],
    };
    let counted = counted.iter().map(|count| (count.standing, count.runs));
    let says = [(Standing::NeedsYou, 1), (Standing::Stopped, 2)];
    assert!(counted.eq(says), "{drawn}");
    let subject = screen.attention.and_then(|attention| attention.subject);
    assert_eq!(subject.as_deref(), Some(GATE.handle()), "{drawn}");
    assert!(pointed_row(&state).contains(GATE.handle()), "{drawn}");
}

/// With no news, the field drawn a second later is the same field, each age a second older.
#[tokio::test]
async fn with_no_news_two_drawings_a_second_apart_differ_only_in_their_ages() {
    let (state, world) = on_the_field(&[&FIRST]).await;
    woken(stopped_beside(&world, &GATE, GATED).await).await;
    let (mut state, _) = drive(state, Vec::new(), &performer_in(&world), &[]).await;
    let mut drawn_at = |seconds: i64| {
        state.update(Input::Time(fixed_now() + TimeDelta::seconds(seconds)));
        Markdown.screen(&state.screen(&Markdown.look()))
    };
    let before = drawn_at(17);
    let after = drawn_at(18);
    assert_ne!(before, after, "the ages move with the clock: {after}");
    assert_eq!(before.replace("17s", "18s"), after);
}

/// A look at the log that failed is said over the field as it was last read, and goes with
/// the next news.
#[tokio::test]
async fn a_look_that_failed_is_said_over_the_field_until_the_next_news() {
    let (mut state, world) = on_the_field(&[&FIRST]).await;
    let failed = "the log could not be read".to_string();
    state.update(Input::LookFailed(failed.clone()));
    let shown = Markdown.draw(&state.screen(&Markdown.look()).main.doc);
    assert!(shown.contains(&failed), "{shown}");
    assert!(shown.contains(FIRST.handle()), "{shown}");

    let performer = performer_in(&world);
    let (inputs, mut received) = mpsc::unbounded_channel();
    performer.take(Request::Follow(state.followed()), &inputs);
    let news = received.recv().await.expect("the field's news");
    state.update(news);
    let shown = Markdown.draw(&state.screen(&Markdown.look()).main.doc);
    assert!(!shown.contains(&failed), "{shown}");
    assert!(shown.contains(FIRST.handle()), "{shown}");
}
