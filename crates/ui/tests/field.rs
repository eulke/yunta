//! The field, through `State` alone: picking runs, closing them after one confirmation
//! naming each, which holds the runs it named while rows move; its keys (`e` the machine's
//! reach, `a` the run the attention row names, `esc` nothing, `r` a resume where the run's
//! acts hold one and a note where it waits on a decision); what the field shows and asks
//! while a person moves through it, and the field drawn from what is followed: its runs
//! moving to their groups with no stroke, its ages at the clock, its reach followed, a look
//! that failed said over it; and its screen's frame: the trail naming the repository, the
//! attention row naming the run that waited longest on a decision answered here, and the
//! note a run's `q` leaves in its place; and its legend, by whether a run stands in sight.

#[path = "field/keys.rs"]
mod keys;
#[path = "field/legend.rs"]
mod legend;
mod support;

use chrono::TimeDelta;
use surco::components::{Answer, Keys, Tally};
use surco::doc::Block;
use surco::surface::{Markdown, Surface};
use surco::words::Standing;
use tokio::sync::mpsc;
use yunta_api::{FollowNews, RunNews, Stop};
use yunta_core::RunId;
use yunta_render::page::FieldRow;
use yunta_render::screen::Leaving;
use yunta_testkit::workflows::GATED;
use yunta_testkit::Bench;
use yunta_testkit_core::fixed_now;
use yunta_testkit_ui::cut::{Built, Cut, Shown};
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::screens::field;
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
    assert_eq!(overlay.title, "close AAAAAA and BBBBBB?");

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

/// A confirmation holds the runs it named: news that move a third run into needs you, above
/// the two picked, while it stands leave its runs as they were, and confirming it closes
/// exactly those two.
#[tokio::test]
async fn a_confirmation_closes_the_runs_it_named_while_rows_move() {
    let (state, world) = on_the_field(&[&FIRST, &SECOND]).await;
    let gate = stopped_beside(&world, &GATE, GATED).await;
    let performer = performer_in(&world);
    let picked = [
        Stroke::Insert(' '),
        Stroke::Insert('j'),
        Stroke::Insert(' '),
        Stroke::Insert('x'),
    ];
    let (state, _) = drive(state, Vec::new(), &performer, &picked).await;
    let look = Markdown.look();
    let asks = |state: &State| state.screen(&look).overlay.map(|overlay| overlay.title);
    assert_eq!(asks(&state).as_deref(), Some("close AAAAAA and BBBBBB?"));

    woken(gate).await;
    let (mut state, _) = drive(state, Vec::new(), &performer, &[]).await;
    let field = Markdown.draw(&state.screen(&look).main.doc);
    let needs_you = field
        .split("needs you, oldest first")
        .nth(1)
        .and_then(|below| below.split(Standing::Stopped.word()).next())
        .unwrap_or_default();
    assert!(needs_you.contains(GATE.handle()), "{field}");
    assert_eq!(asks(&state).as_deref(), Some("close AAAAAA and BBBBBB?"));

    let asked = pressed(&mut state, &[Stroke::Enter]);
    let [Request::Perform(_, Operation::Close { runs })] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!(runs, &[FIRST, SECOND]);
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
    let noted = state.screen(&Markdown.look()).noted;
    let noted = noted.expect("what failed is noted in the attention row's place");
    assert_eq!(noted.said, failed);
}

/// A close of several runs of which more than one is refused notes how many were not closed
/// and why the first was not, in one sentence: not the first refusal alone.
#[tokio::test]
async fn a_close_of_several_refused_notes_how_many_were_not_closed() {
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
    let refusal = |run: &yunta_core::RunId| yunta_render::refusal::Refused::Close {
        run: run.clone(),
        refusal: yunta_core::refusal::CloseRefusal::AlreadyClosed,
    };
    let refused = Outcome::Closed {
        closed: Vec::new(),
        refused: vec![refusal(&FIRST), refusal(&SECOND)],
        failed: None,
    };
    state.update(Input::Outcome(*closing, refused));
    let noted = state.screen(&Markdown.look()).noted;
    let noted = noted.expect("what refused is noted in the attention row's place");
    assert_eq!(
        noted.said,
        "2 runs were not closed, the first because the run is already closed"
    );
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
    let says = [(Standing::NeedsYou, 1)];
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

/// The field over a run asking a person at its own terminal and two that wait on a decision
/// answered here, the one asking having waited longest of the three.
async fn asked_at_a_terminal_and_two_decisions() -> field::FieldScreen {
    let cut = Cut::of(&[
        Shown::at(Built::AskedElsewhere, 6).asking("ship"),
        Shown::at(Built::Blocked, 6),
        Shown::at(Built::Designed, 263),
    ])
    .await;
    field::over(cut).await
}

/// Of `field`'s runs, the one asking at its terminal and the older of the two that wait on a
/// decision: the asking one waited longest of the three.
fn asked_and_older(field: &field::FieldScreen) -> (String, String) {
    let kept = field.state().followed_runs().field.as_ref();
    let rows = kept.expect("the field is read").rows(field.cut().now());
    let asked = rows.iter().find(|row| row.asking());
    let asked = asked.expect("a run asks at its own terminal");
    let waiting: Vec<&FieldRow> = rows.iter().filter(|row| row.menu).collect();
    assert_eq!(waiting.len(), 2, "two runs wait on a decision");
    assert!(
        waiting.iter().all(|row| row.stood() < asked.stood()),
        "the run asking at its terminal waited longest"
    );
    let older = waiting.iter().max_by_key(|row| row.stood());
    let older = older.expect("a run waits on a decision");
    let handle = |row: &FieldRow| row.frame.run_id.handle().to_string();
    (handle(asked), handle(older))
}

/// The field's frame over a run asking at its own terminal and two that wait on a decision,
/// the one asking having waited longest: the attention row names the older of the two, with
/// the key that goes there, and never the run asking at its terminal, which the tally counts
/// among those that need the person; the trail names the repository the field is of.
#[tokio::test]
async fn the_attention_row_names_the_oldest_decision_never_a_run_asking_at_its_terminal() {
    let field = asked_at_a_terminal_and_two_decisions().await;
    let (asked, older) = asked_and_older(&field);
    let screen = field.state().screen(&Markdown.look());
    let place = field.cut().repository().display().to_string();
    assert_eq!(screen.trail, ["yunta", place.as_str()]);
    let Tally::Counted(counts) = &screen.tally else {
        panic!("the field counts its runs");
    };
    let needs = counts
        .iter()
        .find(|count| count.standing == Standing::NeedsYou);
    assert_eq!(
        needs.map(|count| count.runs),
        Some(3),
        "the tally counts all three"
    );
    let attention = screen.attention.expect("a run needs the person");
    assert_eq!(attention.subject, Some(older), "never `{asked}`");
    assert!(
        matches!(attention.answer, Answer::Key(_)),
        "the key that goes there"
    );
}

/// A run's `q` leaves its note on the field in the attention row's place, which a piece of
/// news leaves standing and the next key takes away.
#[tokio::test]
async fn a_runs_q_leaves_its_note_in_the_attention_rows_place_until_the_next_key() {
    let mut field = asked_at_a_terminal_and_two_decisions().await;
    let (_, older) = asked_and_older(&field);
    while !pointed_row(field.state()).contains(&older) {
        field.pressed(&[Stroke::Insert('j')]).await;
    }
    field.pressed(&[Stroke::Enter, Stroke::Insert('q')]).await;
    let noted = |state: &State| state.screen(&Markdown.look()).noted.map(|noted| noted.said);
    let waits = Some(Leaving::Waits.said());
    assert_eq!(
        noted(field.state()).as_deref(),
        waits,
        "leaving the run notes it"
    );
    field.handed(FollowNews::Elsewhere(0));
    assert_eq!(
        noted(field.state()).as_deref(),
        waits,
        "news leave the note standing"
    );
    field.pressed(&[Stroke::Insert('j')]).await;
    assert_eq!(noted(field.state()), None, "the next key takes it away");
}

/// The keys the glance beside the field's list offers on the run pointed at, in its order:
/// none where no glance stands.
fn glance_keys(state: &State) -> Vec<String> {
    let screen = state.screen(&Markdown.look());
    let Some(side) = screen.side else {
        return Vec::new();
    };
    let acts = side
        .region
        .doc
        .blocks()
        .iter()
        .find_map(|block| match block {
            Block::AtAGlance(glance) => glance.acts.as_ref().map(|(_, keys)| keys.clone()),
            _ => None,
        });
    let keys = acts
        .unwrap_or_default()
        .into_iter()
        .map(|binding| binding.keys);
    keys.filter_map(|keys| match keys {
        Keys::Typed(key) => Some(key),
        _ => None,
    })
    .collect()
}

/// How many times the field's kept run `handle` has worked out its acts.
fn acts_worked_out(state: &State, handle: &str) -> u64 {
    let field = state
        .followed_runs()
        .field
        .as_ref()
        .expect("the field is read");
    let run = field.runs.values().find(|run| run.run().handle() == handle);
    run.unwrap_or_else(|| panic!("`{handle}` stands in the field"))
        .acts_worked_out()
}

/// The glance offers the acts of the run pointed at, each by the field's key for its step:
/// a stalled run is gone on from its log with `r` and closed with `x`; a run asked at its own
/// terminal is stopped with `c`, and never closed. Its acts are worked out once for the run's
/// head: drawing the field again, and news of the same head, work out none.
#[tokio::test]
async fn the_glance_offers_the_acts_of_the_run_pointed_at_worked_out_once_a_head() {
    let mut stalled = field::opened("field-glance-stalled").await;
    assert_eq!(glance_keys(stalled.state()), ["r", "x"]);
    let asking = field::opened("field").await;
    let keys = glance_keys(asking.state());
    assert_eq!(
        keys,
        ["c", "s"],
        "a run asked at its terminal offers no `x`"
    );

    let before = acts_worked_out(stalled.state(), "EKPBGE");
    for _ in 0..3 {
        stalled.state().screen(&Markdown.look());
    }
    let field = stalled.state().followed_runs().field.as_ref();
    let run = field.and_then(|field| {
        field
            .runs
            .values()
            .find(|run| run.run().handle() == "EKPBGE")
    });
    let run = run.expect("the stalled run stands in the field");
    let same = RunNews::Engine(run.engine().clone());
    let news = FollowNews::Run {
        run: run.run().clone(),
        news: same,
    };
    stalled.handed(news);
    stalled.state().screen(&Markdown.look());
    assert_eq!(
        acts_worked_out(stalled.state(), "EKPBGE"),
        before,
        "neither a drawing nor news of the same head works its acts out again"
    );
}
