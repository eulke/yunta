//! A run's screen in its frame, through `State` alone, on the cuts of the design's targets:
//! the tally at the trail's far end counts the runs of the field the run stands in, a run
//! asked at its own terminal among them; the attention row names the run's own decision
//! where it is out of sight, and elsewhere the run that waited longest on the person among
//! those answered here, never one asked at its own terminal, with no key where a line is
//! typed and none at all where the decision stands in sight; the legend ends with the far
//! keys where no line is typed; and the note stands in the attention row's place until the
//! next key, through news, what an act refused or failed on, a close's among them. Each row is held to its target's, drawn at the design's widest
//! screen.

use surco::components::{Attention, Screen};
use surco::glyphs::{Glyphs, Mark};
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_api::{FollowNews, Remark, RunNews};
use yunta_core::Pid;
use yunta_render::refusal::Refused;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Outcome, Request, State};

/// The cells inside the margins of the design's widest screen, 120 columns.
const WIDEST: usize = 118;

/// The cells inside the margins of a terminal of 80 columns.
const NARROWER: usize = 78;

/// The look a frame's rows are drawn in: Unicode, with no paint, `cells` wide.
fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: Motion::Still,
    }
}

/// `state`'s screen.
fn screen(state: &State) -> Screen {
    state.screen(&look(WIDEST))
}

/// The trail of `state`'s screen with its tally, as a terminal draws its row `cells` wide.
fn trail_row(state: &State, cells: usize) -> String {
    let screen = screen(state);
    let row = Terminal::on(look(cells)).trail(&screen.trail, &screen.tally);
    row.text().trim_end().to_string()
}

/// The attention row of `state`'s screen as a terminal draws it `cells` wide; none where
/// none stands.
fn attention_row(state: &State, cells: usize) -> Option<String> {
    let attention: Attention = screen(state).attention?;
    let row = Terminal::on(look(cells)).attention(&attention);
    Some(row.text().trim_end().to_string())
}

/// The note of `state`'s screen, its mark and what it says; none where none stands.
fn note(state: &State) -> Option<(Mark, String)> {
    screen(state).noted.map(|noted| (noted.mark, noted.said))
}

/// The keys' row of `state`'s screen, the far keys at its end, as a terminal draws it
/// `cells` wide.
fn legend_row(state: &State, cells: usize) -> String {
    let screen = screen(state);
    let row = Terminal::on(look(cells)).legend(&screen.legend, &screen.anywhere);
    row.text().trim_end().to_string()
}

/// The row a target draws at its first column, its margin's cell left out.
fn target(row: &str) -> String {
    row.strip_prefix(' ').unwrap_or(row).trim_end().to_string()
}

/// `failure.unicode-120` row 1: the design's run on its suite's failure, beside a run that
/// needs the person and one stalled.
const FAILURE_TRAIL: &str = " yunta › Q78GRR › tests                                                                      ◆ 2 need you · ▲ 1 stalled";

/// `failure.unicode-80` row 1.
const FAILURE_TRAIL_80: &str =
    " yunta › Q78GRR › tests                              ◆ 2 need you · ▲ 1 stalled";

/// `work.unicode-120` row 1: the design's run at work, beside the same two.
const WORK_TRAIL: &str = " yunta › Q78GRR › plan                                                                      ◆ 1 needs you · ▲ 1 stalled";

/// `asked-elsewhere.unicode-120` row 1: a run asking at its own terminal, counted.
const ASKED_ELSEWHERE_TRAIL: &str = " yunta › ASKERS › ship                                                                       ◆ 2 need you · ▲ 1 stalled";

/// `work.unicode-120` row 35, the attention row of every target whose run's own decision is
/// not in sight and whose run leads: the run beside it that waited longest on the person.
const LONGEST_WAITING: &str = " ◆ EZRPE9 needs you: node `grill` failed: session `mock` refused the request (its credential was refused…  · a go there";

/// `palette.unicode-120` row 35: the same run, where the palette's line is typed.
const LONGEST_WAITING_TYPED: &str = " ◆ EZRPE9 needs you: node `grill` failed: session `mock` refused the request (its credential was refused): Not logged…";

/// `failure.unicode-120` row 6, the design's run's gist on its suite's failure.
const FAILURE_GIST: &str =
    "regression: `cargo test --workspace` passed at baseline (exit 0) but now exits 101";

/// The design's gist of the run parked on its scope, as `scope.unicode-120` row 6 says it:
/// one state has one gist. `node-needs-you`'s attention row stands at the same cut, and kept
/// the base's words from its dump.
const SCOPE_GIST: &str = "a task asks to touch a path outside its scope";

/// `work.unicode-120` row 36's far keys.
const FAR_KEYS: &str = "/ anything · ? keys";

#[tokio::test]
async fn the_tally_counts_the_runs_of_the_field_the_run_stands_in() {
    let failure = run::opened("failure").await;
    assert_eq!(trail_row(failure.state(), WIDEST), target(FAILURE_TRAIL));
    assert_eq!(
        trail_row(failure.state(), NARROWER),
        target(FAILURE_TRAIL_80)
    );
    let work = run::opened("work").await;
    assert_eq!(trail_row(work.state(), WIDEST), target(WORK_TRAIL));
}

#[tokio::test]
async fn the_runs_own_decision_out_of_sight_is_the_attention_row() {
    let mut failure = run::opened("failure").await;
    assert_eq!(
        attention_row(failure.state(), WIDEST),
        None,
        "the decision stands in sight, as its card"
    );
    failure.pressed(&[Stroke::Insert('j')]).await;
    assert_eq!(
        attention_row(failure.state(), WIDEST),
        None,
        "`j` chooses on the card in place, which stays in sight"
    );
    failure.pressed(&[Stroke::Insert('s')]).await;
    assert_eq!(
        attention_row(failure.state(), WIDEST),
        Some(format!("◆ Q78GRR needs you: {FAILURE_GIST}  · a go there")),
        "its numbers stand over the run: the run's own decision is out of sight"
    );
    let scope = run::opened("node-needs-you").await;
    assert_eq!(
        attention_row(scope.state(), WIDEST),
        Some(format!("◆ Q78GRR needs you: {SCOPE_GIST}  · a go there")),
        "the person leads: the run's own decision is out of sight"
    );
}

#[tokio::test]
async fn elsewhere_the_attention_row_names_the_run_that_waited_longest() {
    // The run leads at work: nothing of its own needs the person.
    let work = run::opened("work").await;
    assert_eq!(
        attention_row(work.state(), WIDEST),
        Some(target(LONGEST_WAITING))
    );
    // The run leads on a pause whose ask is not read yet: what it asks is its pane's once
    // read.
    let unread = run::opened("still-unread").await;
    assert_eq!(
        attention_row(unread.state(), WIDEST),
        Some(target(LONGEST_WAITING))
    );
}

#[tokio::test]
async fn a_run_asked_at_its_own_terminal_is_counted_and_never_named() {
    let mut asking = run::opened("asked-elsewhere").await;
    assert_eq!(
        trail_row(asking.state(), WIDEST),
        target(ASKED_ELSEWHERE_TRAIL)
    );
    assert_eq!(
        attention_row(asking.state(), WIDEST),
        Some(target(LONGEST_WAITING))
    );
    asking.pressed(&[Stroke::Insert('j')]).await;
    assert_eq!(
        attention_row(asking.state(), WIDEST),
        Some(target(LONGEST_WAITING)),
        "the person leads, and the run asking at its terminal is still not named"
    );
}

#[tokio::test]
async fn the_palette_over_the_run_leaves_the_attention_row_with_no_key() {
    let palette = run::opened("palette").await;
    assert_eq!(
        attention_row(palette.state(), WIDEST),
        Some(target(LONGEST_WAITING_TYPED))
    );
    assert!(
        screen(palette.state()).anywhere.0.is_empty(),
        "no far keys where a line is typed"
    );
}

#[tokio::test]
async fn the_legend_ends_with_the_far_keys_where_no_line_is_typed() {
    let work = run::opened("work").await;
    let row = legend_row(work.state(), WIDEST);
    assert!(row.ends_with(FAR_KEYS), "{row}");
    assert_eq!(row.matches("? ").count(), 1, "help's key once: {row}");
}

/// The resume `stroke` asks for on `state`, answered `refused` in place of the work.
fn refused_on(state: &mut State, stroke: Stroke, refused: Refused) {
    let asked = state.update(Input::Stroke(stroke));
    let Some(ticket) = asked.iter().find_map(|request| match request {
        Request::Perform(ticket, Operation::Resume { .. }) => Some(*ticket),
        _ => None,
    }) else {
        panic!("the key asks to resume the run: {asked:?}");
    };
    state.update(Input::Outcome(ticket, Outcome::Refused(refused)));
}

/// The close `x` asks for on `state`, once confirmed, answered `closing` in place of the work.
fn closed_on(state: &mut State, closing: Outcome) {
    state.update(Input::Stroke(Stroke::Insert('x')));
    let asked = state.update(Input::Stroke(Stroke::Enter));
    let Some(ticket) = asked.iter().find_map(|request| match request {
        Request::Perform(ticket, Operation::Close { .. }) => Some(*ticket),
        _ => None,
    }) else {
        panic!("the key asks to close the run once confirmed: {asked:?}");
    };
    state.update(Input::Outcome(ticket, closing));
}

/// A close refused or failed on the run's own screen is noted there, as a stop's is: why it
/// refused, or what failed below Yunta.
#[tokio::test]
async fn a_close_refused_or_failed_on_the_runs_screen_is_noted_there() {
    let stopped = run::opened("stopped").await;
    let run = stopped
        .state()
        .followed()
        .run
        .expect("the run's screen follows its run");
    let refused = Refused::Moving {
        run,
        pid: Pid::current(),
    };
    let said = refused.to_string();
    let mut state = stopped.state().clone();
    let refusing = Outcome::Closed {
        closed: Vec::new(),
        refused: vec![refused],
        failed: None,
    };
    closed_on(&mut state, refusing);
    assert_eq!(note(&state), Some((Mark::Done, said)));
    let mut state = stopped.state().clone();
    let failed = "the disk refused the close".to_string();
    let failing = Outcome::Closed {
        closed: Vec::new(),
        refused: Vec::new(),
        failed: Some(failed.clone()),
    };
    closed_on(&mut state, failing);
    assert_eq!(note(&state), Some((Mark::Failed, failed)));
}

#[tokio::test]
async fn a_refused_acts_note_stands_through_news_and_goes_at_the_next_key() {
    let stopped = run::opened("stopped").await;
    let mut state = stopped.state().clone();
    let run = state
        .followed()
        .run
        .expect("the run's screen follows its run");
    let refused = Refused::Moving {
        run: run.clone(),
        pid: Pid::current(),
    };
    let said = refused.to_string();
    refused_on(&mut state, Stroke::Insert('r'), refused);
    assert_eq!(note(&state), Some((Mark::Done, said.clone())));

    let news = RunNews::Events(Vec::new());
    state.update(Input::FollowNews(FollowNews::Run { run, news }));
    assert_eq!(
        note(&state),
        Some((Mark::Done, said)),
        "news leave it standing"
    );
    state.update(Input::Stroke(Stroke::Insert('z')));
    assert_eq!(note(&state), None, "the next key takes it away");
}

#[tokio::test]
async fn a_remark_kept_for_the_run_is_the_note_once() {
    let mut work: RunScreen = run::opened("work").await;
    let remark = Remark::UnreadManifestKeys {
        keys: vec!["horizon".to_string()],
    };
    let words = yunta_ui::remark::said(&remark, Glyphs::Unicode).words;
    work.handed(RunNews::Remark(remark.clone()));
    let said = note(work.state()).map(|(_, said)| said);
    assert_eq!(said, Some(words), "said once the run's screen draws");

    work.pressed(&[Stroke::Insert('z')]).await;
    assert_eq!(note(work.state()), None, "gone after the next key");
    work.handed(RunNews::Remark(remark));
    assert_eq!(note(work.state()), None, "a remark kept is said once");
}

#[tokio::test]
async fn remarks_that_come_together_are_said_one_at_a_time() {
    let mut work: RunScreen = run::opened("work").await;
    let remarks = ["horizon", "furrows"].map(|key| Remark::UnreadManifestKeys {
        keys: vec![key.to_string()],
    });
    let words = remarks
        .clone()
        .map(|remark| yunta_ui::remark::said(&remark, Glyphs::Unicode).words);
    for remark in remarks {
        work.handed(RunNews::Remark(remark));
    }
    let said = |work: &RunScreen| note(work.state()).map(|(_, said)| said);
    assert_eq!(said(&work), Some(words[0].clone()), "the first that came");
    work.pressed(&[Stroke::Insert('z')]).await;
    assert_eq!(said(&work), Some(words[1].clone()), "then the next");
    work.pressed(&[Stroke::Insert('z')]).await;
    assert_eq!(said(&work), None, "each once");
}
