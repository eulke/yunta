//! A round of questions in place on a run's screen, through `State` alone, on the design's
//! run where `grill` asks three: it stands by itself with no key pressed, the first question
//! in turn; nothing is recorded until its last answer, which records every answer at once and
//! is noted with how many and what the run does with them; `shift-tab` goes back to the
//! question before with its answer kept; `esc` puts it aside with nothing recorded, the node
//! it waits at pointed by the person and the note saying so, and `f` brings it back as the
//! person left it, whatever node they read meanwhile. On a run whose answers the
//! engine refused, the refusal stands under its question while the person types, and the
//! answers given then are recorded. A round of one question on a run a bench parks is
//! `one_question`'s.

use surco::components::{CardBody, RoundRow, RoundStanding};
use surco::doc::Block;
use surco::glyphs::{Glyphs, Mark};
use surco::look::{Look, Width};
use surco::surface::Terminal;
use yunta_core::events::{EventPayload, GateEvent};
use yunta_core::RunId;
use yunta_storage::Storage;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Operation, Scene};

/// Where a question of a round stands, as a test reads it.
#[derive(Debug, PartialEq, Eq)]
enum Turn {
    /// Answered, with the answer as a reader reads it.
    Answered(String),
    /// Being typed into: the words before the cursor, and what was refused under it.
    Typing {
        before: String,
        refused: Vec<String>,
    },
    /// Being picked from a list, its pointer on the row `at`.
    Picking { at: Option<usize> },
    /// Still to come.
    Ahead,
}

/// What a test reads of a round in place: its question, its far edge, whether it says keys
/// of its own, and where each of its questions stands.
#[derive(Debug)]
struct ReadRound {
    question: String,
    edge: Option<String>,
    keys_of_its_own: bool,
    turns: Vec<Turn>,
}

/// The round of questions standing last in the main region of `screen`, under the request
/// it is about where the run has one, as a test reads it: none where none stands.
fn round(screen: &RunScreen) -> Option<ReadRound> {
    let main = screen.state().screen(&Look::plain()).main;
    let Some(Block::Card(card)) = main.doc.blocks().last() else {
        return None;
    };
    let CardBody::Round(rows) = &card.body else {
        return None;
    };
    Some(ReadRound {
        question: card.question.clone(),
        edge: card.edge.clone(),
        keys_of_its_own: !card.legend.is_empty(),
        turns: rows.iter().map(turn).collect(),
    })
}

/// Where `row` stands, as a test reads it.
fn turn(row: &RoundRow) -> Turn {
    match &row.stands {
        RoundStanding::Answered(said) => Turn::Answered(said.clone()),
        RoundStanding::Typing { typed, refused } => Turn::Typing {
            before: typed.before.clone(),
            refused: refused.clone(),
        },
        RoundStanding::Picking { choices, .. } => Turn::Picking {
            at: choices.as_ref().map(|choices| choices.at),
        },
        RoundStanding::Ahead => Turn::Ahead,
    }
}

/// Where each question of the round standing in `screen` stands.
fn turns(screen: &RunScreen) -> Vec<Turn> {
    round(screen).expect("the round stands in place").turns
}

/// How many times `screen`'s run had its questions answered on its log.
fn answered(screen: &RunScreen) -> usize {
    let run: RunId = screen.state().followed().run.expect("a run's screen");
    let storage = Storage::open(&screen.store()).expect("the cut's log");
    let events = storage.events_for_run(&run).expect("the run's log");
    events
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Gates(GateEvent::QuestionsAnswered(_)))
            )
        })
        .count()
}

/// The answers `screen`'s application asked its performer to record, each a question's id
/// and its value, a recording each, in the order asked: none where it asked for none.
fn asked_to_record(screen: &RunScreen) -> Vec<Vec<(String, String)>> {
    screen
        .performed()
        .iter()
        .filter_map(|work| match work {
            Operation::Answer { answers, .. } => Some(
                answers
                    .iter()
                    .map(|answer| (answer.id.to_string(), answer.value.clone()))
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

/// The legend `screen` stands with, as Unicode spells it.
fn legend(screen: &RunScreen) -> String {
    let legend = screen.state().screen(&Look::plain()).legend;
    super::support::spelled(&legend, Glyphs::Unicode)
}

#[tokio::test]
async fn the_round_stands_in_place_with_no_key_pressed_its_first_question_in_turn() {
    let screen = run::opened("round").await;
    assert!(matches!(screen.state().scene(), Some(Scene::Round)));
    let round = round(&screen).expect("the round stands in place");
    assert_eq!(round.question, "grill asks");
    assert_eq!(round.edge.as_deref(), Some("Q78GRR"));
    assert!(
        !round.keys_of_its_own,
        "its keys stand in the screen's legend"
    );
    assert_eq!(
        round.turns,
        [Turn::Picking { at: Some(0) }, Turn::Ahead, Turn::Ahead]
    );
    assert_eq!(
        legend(&screen),
        "↑↓ choose · enter answer · esc leave it for later"
    );
}

/// The rows the round's region stands in on the design's screen at 120 by 36, and at the
/// least terminal, 60 by 14: what its frame leaves of each.
const ROUND_ROWS: [(usize, usize); 2] = [(118, 24), (58, 6)];

/// On the design's round, whose first question is picked from a list, every printable key
/// types on the row the list is narrowed on, so the cursor stands there, on the first cell
/// of `type to filter`, at the design's size and at the least.
#[tokio::test]
async fn the_cursor_stands_where_typing_lands_on_a_round_of_choices() {
    let screen = run::opened("round").await;
    for (cells, rows) in ROUND_ROWS {
        let look = Look {
            width: Width::exact(cells),
            ..Look::plain()
        };
        let shown = screen.state().screen(&look);
        let paned = Terminal::on(look).pane(&shown.main, rows);
        let (row, cell) = paned.cursor.expect("a cursor on a round of choices");
        let from: String = paned.lines[row].text().chars().skip(cell).collect();
        assert!(
            from.starts_with("type to filter"),
            "{cells} cells: the cursor at {cell} of {:?}",
            paned.lines[row].text()
        );
    }
}

#[tokio::test]
async fn nothing_is_recorded_until_the_last_answer_which_records_every_one() {
    let mut screen = run::opened("round").await;
    let before = answered(&screen);
    screen.pressed(&[Stroke::Enter]).await;
    screen.pressed(&[Stroke::Enter]).await;
    assert_eq!(answered(&screen), before, "two answers record nothing");
    assert!(
        asked_to_record(&screen).is_empty(),
        "{:?}",
        screen.performed()
    );
    screen.pressed(&[Stroke::Enter]).await;
    assert_eq!(
        answered(&screen),
        before + 1,
        "the last answer records the round"
    );
    let asked = asked_to_record(&screen);
    let [recorded] = asked.as_slice() else {
        panic!("one recording asked: {asked:?}");
    };
    let ids: Vec<&str> = recorded.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "where-global-lives",
            "how-requested",
            "beside-project-packs"
        ]
    );
    let noted = screen.state().screen(&Look::plain()).noted;
    let noted = noted.expect("the answers recorded are noted");
    assert_eq!(noted.mark, Mark::Done, "{}", noted.said);
    assert_eq!(
        noted.said,
        "Recorded: 3 answers. `grill` finishes with them, and the run goes on."
    );
}

#[tokio::test]
async fn shift_tab_from_the_second_question_goes_back_to_the_first_with_its_answer_kept() {
    let mut screen = run::opened("round").await;
    screen.pressed(&[Stroke::Down, Stroke::Enter]).await;
    let picked = "A fixed home-directory path such as ~/.yunta".to_string();
    assert_eq!(
        turns(&screen),
        [
            Turn::Answered(picked),
            Turn::Picking { at: Some(0) },
            Turn::Ahead
        ]
    );
    assert!(
        legend(&screen).contains("shift-tab go back"),
        "{}",
        legend(&screen)
    );
    screen.pressed(&[Stroke::BackTab]).await;
    assert_eq!(
        turns(&screen),
        [Turn::Picking { at: Some(1) }, Turn::Ahead, Turn::Ahead],
        "the first question in turn again, its answer kept"
    );
    assert!(
        asked_to_record(&screen).is_empty(),
        "{:?}",
        screen.performed()
    );
}

#[tokio::test]
async fn esc_puts_the_round_aside_with_nothing_recorded_and_says_so() {
    let mut screen = run::opened("round").await;
    let before = answered(&screen);
    screen.pressed(&[Stroke::Enter, Stroke::Decline]).await;
    assert!(
        matches!(screen.state().scene(), Some(Scene::Node)),
        "the node it waits at stands as its own scene"
    );
    let drawn = screen.state().screen(&Look::plain());
    assert_eq!(drawn.trail.last().map(String::as_str), Some("grill"));
    let noted = drawn.noted.expect("putting it aside is noted");
    assert_eq!(noted.mark, Mark::Done, "{}", noted.said);
    assert_eq!(
        noted.said,
        "Nothing recorded. The run stays waiting on these questions."
    );
    assert_eq!(answered(&screen), before, "nothing recorded");
    assert!(
        asked_to_record(&screen).is_empty(),
        "{:?}",
        screen.performed()
    );
}

#[tokio::test]
async fn f_brings_the_round_put_aside_back_as_it_was_left() {
    let mut screen = run::opened("round").await;
    screen
        .pressed(&[Stroke::Down, Stroke::Enter, Stroke::Down])
        .await;
    let left = turns(&screen);
    let picked = "A fixed home-directory path such as ~/.yunta".to_string();
    assert_eq!(
        left,
        [
            Turn::Answered(picked),
            Turn::Picking { at: Some(1) },
            Turn::Ahead
        ]
    );
    screen
        .pressed(&[Stroke::Decline, Stroke::Insert('j')])
        .await;
    let drawn = screen.state().screen(&Look::plain());
    assert_eq!(drawn.trail.last().map(String::as_str), Some("brief"));
    screen.pressed(&[Stroke::Insert('f')]).await;
    assert!(
        matches!(screen.state().scene(), Some(Scene::Round)),
        "{:?}",
        screen.state().scene()
    );
    assert_eq!(turns(&screen), left, "as the person left it");
    assert!(
        asked_to_record(&screen).is_empty(),
        "{:?}",
        screen.performed()
    );
}

#[tokio::test]
async fn on_a_refused_round_the_refusal_stands_under_its_question_and_the_next_answers_are_recorded(
) {
    let mut screen = run::opened("round-refused").await;
    assert!(matches!(screen.state().scene(), Some(Scene::Round)));
    let refusal = "this question is `required` and nothing answers it".to_string();
    assert_eq!(
        turns(&screen),
        [
            Turn::Typing {
                before: "Spring release".to_string(),
                refused: vec![refusal],
            },
            Turn::Ahead
        ],
        "the engine's refusal stands under its question while the person types"
    );
    let before = answered(&screen);
    screen.pressed(&[Stroke::Enter, Stroke::Enter]).await;
    assert_eq!(answered(&screen), before + 1, "{:?}", screen.performed());
    assert_eq!(
        asked_to_record(&screen),
        [vec![
            ("q1".to_string(), "Spring release".to_string()),
            ("q2".to_string(), "staging".to_string()),
        ]]
    );
}
