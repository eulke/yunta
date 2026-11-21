//! A decision a run waits on, in place on the run's screen, through `State` alone, on the
//! design's run at its last gate: the card stands at the pane's foot by itself while the run
//! leads, under what it rests on; the arrows move what stands above it and never its choice,
//! and the run still leads; a number points its option; `enter` records the option pointed
//! at on the run's log, once, and the note says what the run does now. On the design's run
//! where a task asks for a path outside its scope, the grant is recorded so and noted; where
//! a check failed, its card keeps both options on the least terminal, what the check printed
//! cut from its top above them. Help over a card says each key the card names in the card's
//! words, and no row of the run's for those keys. A round of questions in place is
//! `round`'s, one of one question on a run a bench parks `one_question`'s, and the plan's
//! desk `desk`'s, the plan read from a card among it; a prompt put aside and brought back is
//! `aside`'s, and an answer given elsewhere `elsewhere`'s.

mod support;

#[path = "in_place/aside.rs"]
mod aside;
#[path = "in_place/desk.rs"]
mod desk;
#[path = "in_place/elsewhere.rs"]
mod elsewhere;
#[path = "in_place/one_question.rs"]
mod one_question;
#[path = "in_place/round.rs"]
mod round;

use surco::components::{Card, CardBody, ScreenLaid};
use surco::doc::Block;
use surco::glyphs::{Glyphs, Mark};
use surco::line::Line;
use surco::look::{Look, Width};
use surco::surface::Terminal;
use yunta_core::events::{EventPayload, GateEvent, GateResolvedPayload};
use yunta_core::RunId;
use yunta_storage::Storage;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Grounds, Scene, ScrolledTo};

/// The cells and the rows the pane of a run's screen holds under its title on the least
/// terminal, 60 by 14: what stands above a card cannot all stand there.
const LEAST: (usize, usize) = (58, 5);

/// The cells and the rows the pane of the design's run holds under its title at 80 by 24,
/// the ribbon's strokes alone across it: a card stands whole there and what stands above it
/// gives way.
const AT_80: (usize, usize) = (78, 14);

/// The cells and the rows the pane of a run of two nodes holds under its title at 80 by 24,
/// where the ribbon names them over its strokes.
const AT_80_NAMED: (usize, usize) = (78, 13);

/// The rows of the target `failure` at 80 by 24 under its pane's title, rows 8 to 21, the
/// region's margin left out.
const FAILURE_AT_80: [&str; 14] = [
    "",
    "  │ … 2 above",
    "  │ assertion `left == right` failed: src_files_over_500_lines",
    "  │   left: Some(1)",
    "  │  right: Some(0)",
    "  │ test result: FAILED. 71 passed; 1 failed; 0 ignored; finished in 21.18s",
    "",
    "  kind         check, baseline_compare",
    "  took         32s",
    "▎ ◆ `tests` failed. What now?                                            tests",
    "▎",
    "▎ ▸ retry  Starts a fresh attempt from the run's tree as it stands: what",
    "▎          failed is asked again and what passed stays passed",
    "▎   abort  Pause here: nothing runs; this decision stays open",
];

/// The rows of the target `departure` at 80 by 24 under its pane's title, rows 9 to 21, the
/// region's margin left out.
const DEPARTURE_AT_80: [&str; 13] = [
    "the plan decided",
    "│ Hello, and good night",
    "",
    "the session says instead",
    "│ Hello",
    "▎ ◆ The task departs from its plan. Accept it?                       implement",
    "▎",
    "▎ ▸ continue-work  keeps the blocked task's last changes and judges them",
    "▎                  before any session opens",
    "▎   accept         the work stands as it is, and the plan records why it",
    "▎                  differs",
    "▎   send-back      the session goes back to what the plan says",
    "▎   abort          Pause here: nothing runs; this decision stays open",
];

/// What a test reads of a card: its question, the name of the option its pointer stands
/// on, and whether it says keys of its own.
struct Read {
    question: String,
    pointed: Option<String>,
    keys_of_its_own: bool,
}

/// The card standing at the foot of the main region of `screen`, as a test reads it, where
/// one stands.
fn card(screen: &RunScreen) -> Option<Read> {
    let main = screen.state().screen(&Look::plain()).main;
    match main.doc.blocks().last()? {
        Block::Card(card) => Some(read(card)),
        _ => None,
    }
}

/// `card` as a test reads it.
fn read(card: &Card) -> Read {
    let pointed = match &card.body {
        CardBody::Choosing { choices, .. } => {
            choices.rows.get(choices.at).map(|row| row.name.clone())
        }
        CardBody::Saying { .. } | CardBody::Round(_) => None,
    };
    Read {
        question: card.question.clone(),
        pointed,
        keys_of_its_own: !card.legend.is_empty(),
    }
}

/// Whether `screen` stands as a card on what the gate shows: a card stands only while the
/// run leads.
fn on_the_card(screen: &RunScreen) -> bool {
    resting_on(screen, Grounds::Shown)
}

/// Whether `screen` stands as a card resting on `grounds`.
fn resting_on(screen: &RunScreen, grounds: Grounds) -> bool {
    matches!(screen.state().scene(), Some(Scene::Card(_, on)) if on == grounds)
}

/// `screen`'s main region laid out on the least terminal, the shortest look, and handed back
/// as a terminal hands it, as the next key moves from: each row's words.
fn laid_out(screen: &mut RunScreen) -> Vec<String> {
    let (cells, rows) = LEAST;
    let look = Look {
        width: Width::exact(cells),
        ..Look::plain()
    };
    let main = screen.state().screen(&look).main;
    let paned = Terminal::on(look).pane_short_of_rows(&main, rows);
    screen.laid(ScreenLaid {
        main: paned.laid,
        side: None,
        overlay: None,
    });
    paned.lines.iter().map(Line::text).collect()
}

/// The decisions `screen`'s run has on its log, each the option chosen, in the log's order.
fn decided(screen: &RunScreen) -> Vec<String> {
    let run: RunId = screen.state().followed().run.expect("a run's screen");
    let storage = Storage::open(&screen.store()).expect("the cut's log");
    let events = storage.events_for_run(&run).expect("the run's log");
    events
        .iter()
        .filter_map(|event| match event.payload()? {
            EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(chosen))) => {
                Some(chosen.option.to_string())
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn the_card_stands_at_the_panes_foot_by_itself_while_the_run_leads() {
    let screen = run::opened("ship").await;
    assert!(on_the_card(&screen));
    let card = card(&screen).expect("the card stands at the pane's foot");
    assert_eq!(card.question, "Create the PR?");
    assert_eq!(card.pointed.as_deref(), Some("approve"));
    assert!(
        !card.keys_of_its_own,
        "its keys stand in the screen's legend"
    );
    let legend = screen.state().screen(&Look::plain()).legend;
    let spelled = legend.spelled(surco::glyphs::Glyphs::Unicode);
    assert!(
        spelled.starts_with("j k choose · enter answer"),
        "{spelled}"
    );
}

#[tokio::test]
async fn the_arrows_move_what_stands_above_the_card_and_never_its_choice() {
    let mut screen = run::opened("ship").await;
    let before = laid_out(&mut screen);
    assert!(
        before.iter().any(|row| row.contains("criterion green")),
        "{before:#?}"
    );
    screen.pressed(&[Stroke::Up]).await;
    let after = laid_out(&mut screen);
    assert_ne!(after, before, "what stands above the card moved");
    let pane = screen.state().pane().expect("a run's screen");
    assert!(matches!(pane.to(), ScrolledTo::Row(_)), "{pane:?}");
    let card = card(&screen).expect("the card still stands");
    assert_eq!(card.pointed.as_deref(), Some("approve"));
    assert!(
        on_the_card(&screen),
        "scrolling above a card takes nothing from the run"
    );
    screen.pressed(&[Stroke::Down]).await;
    assert_eq!(
        laid_out(&mut screen),
        before,
        "down goes back to where it stood"
    );
    let card = self::card(&screen).expect("the card still stands");
    assert_eq!(card.pointed.as_deref(), Some("approve"));
}

#[tokio::test]
async fn a_number_points_its_option_and_enter_records_it_once_with_what_the_run_does_now() {
    let mut screen = run::opened("ship").await;
    let before = decided(&screen);
    screen.pressed(&[Stroke::Insert('2')]).await;
    let card = card(&screen).expect("the card stands");
    assert_eq!(card.pointed.as_deref(), Some("abort"));
    assert_eq!(decided(&screen), before, "a number records nothing");
    screen.pressed(&[Stroke::Enter]).await;
    let recorded = decided(&screen);
    assert_eq!(recorded[..before.len()], before);
    assert_eq!(recorded[before.len()..], ["abort"], "recorded once");
    let noted = screen.state().screen(&Look::plain()).noted;
    let noted = noted.expect("the answer recorded is noted");
    assert_eq!(noted.mark, Mark::Done, "{}", noted.said);
    assert_eq!(
        noted.said,
        "Recorded: abort. The run pauses here, and asks again when it is resumed."
    );
}

#[tokio::test]
async fn a_scopes_grant_is_recorded_once_and_noted_with_the_session_going_on() {
    let mut screen = run::opened("scope").await;
    assert!(resting_on(&screen, Grounds::Scope));
    let card = card(&screen).expect("the scope's card stands at the pane's foot");
    assert_eq!(card.pointed.as_deref(), Some("grant"));
    let before = decided(&screen);
    screen.pressed(&[Stroke::Enter]).await;
    let recorded = decided(&screen);
    assert_eq!(recorded[..before.len()], before);
    assert_eq!(recorded[before.len()..], ["grant"], "recorded once");
    let noted = screen.state().screen(&Look::plain()).noted;
    let noted = noted.expect("the answer recorded is noted");
    assert_eq!(noted.mark, Mark::Done, "{}", noted.said);
    assert_eq!(
        noted.said,
        "Recorded: grant. The session goes on with the wider scope."
    );
}

#[tokio::test]
async fn a_failures_card_keeps_both_options_on_the_least_terminal_its_tail_cut_from_the_top() {
    let mut screen = run::opened("failure").await;
    assert!(resting_on(&screen, Grounds::Failure));
    let rows = laid_out(&mut screen);
    let [count, last, question, retry, abort] = rows.as_slice() else {
        panic!("the pane holds five rows: {rows:#?}");
    };
    assert!(question.contains("`tests` failed. What now?"), "{rows:#?}");
    assert!(retry.contains("retry"), "{rows:#?}");
    assert!(abort.contains("abort"), "{rows:#?}");
    assert!(
        last.contains("test result: FAILED"),
        "the tail's last line stands over the card: {rows:#?}"
    );
    assert!(
        count.contains("5 above"),
        "the lines cut from the tail's top are counted: {rows:#?}"
    );
}

/// `screen`'s main region laid out in a pane of `(cells, rows)` under its title, a look
/// taller than the shortest, in Unicode: each row's words, its trailing blanks cut.
fn laid_taller(screen: &RunScreen, (cells, rows): (usize, usize)) -> Vec<String> {
    let look = Look {
        glyphs: Glyphs::Unicode,
        width: Width::exact(cells),
        ..Look::plain()
    };
    let main = screen.state().screen(&look).main;
    let paned = Terminal::on(look).pane(&main, rows);
    let rows = paned.lines.iter().map(Line::text);
    rows.map(|row| row.trim_end().to_string()).collect()
}

#[tokio::test]
async fn above_a_card_standing_whole_a_failure_lets_its_attempt_go_then_its_tail_from_its_top() {
    let screen = run::opened("failure").await;
    assert_eq!(laid_taller(&screen, AT_80), FAILURE_AT_80);
}

#[tokio::test]
async fn above_a_card_standing_whole_a_departure_lets_why_go_then_the_blank_row_under_its_title() {
    let screen = run::opened("departure").await;
    assert_eq!(laid_taller(&screen, AT_80_NAMED), DEPARTURE_AT_80);
}

/// What each row of `rows` that names `key`, alone or beside others, says it does.
fn said_for<'r>(rows: &'r [(String, String)], key: &str) -> Vec<&'r str> {
    rows.iter()
        .filter(|(keys, _)| keys.split_whitespace().any(|named| named == key))
        .map(|(_, does)| does.as_str())
        .collect()
}

/// Each row help lists over `screen`, opened by `?`: its keys, then what each does.
fn help_rows(screen: &mut RunScreen) -> Vec<(String, String)> {
    let overlay = screen.state().screen(&Look::plain()).overlay;
    let overlay = overlay.filter(|overlay| overlay.title == yunta_render::screen::HELP);
    let help = overlay.expect("help stands");
    let listed = help.doc.blocks().iter().find_map(|block| match block {
        Block::KeyList(list) => Some(&list.keys.0),
        _ => None,
    });
    let listed = listed.expect("help lists its keys");
    listed
        .iter()
        .map(|binding| (binding.keys.named().to_string(), binding.does.clone()))
        .collect()
}

#[tokio::test]
async fn help_over_the_scopes_card_says_n_adds_a_note_and_nowhere_starts_a_run() {
    let mut screen = run::opened("scope").await;
    assert!(resting_on(&screen, Grounds::Scope));
    screen.pressed(&[Stroke::Insert('?')]).await;
    let rows = help_rows(&mut screen);
    assert_eq!(said_for(&rows, "n"), ["add a note"], "{rows:#?}");
    assert_eq!(said_for(&rows, "enter"), ["answer"], "{rows:#?}");
    // The keys the card takes beyond its legend's, in its words: they choose too.
    for key in ["tab", "shift-tab", "1", "2"] {
        assert_eq!(said_for(&rows, key), ["choose"], "{key}: {rows:#?}");
    }
}
