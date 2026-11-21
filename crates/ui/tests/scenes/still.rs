//! Work that stands still, through `State` alone: a run that goes on alone once a limit
//! resets turns no spinner; a pause whose ask is not read yet shows its node at work with no
//! card and none of a card's keys; one whose ask could not be read says why where the card
//! stands, and no key answers it; an answer awaiting its resume is offered the resume only
//! where no engine takes it up.

use surco::components::{Keys, Screen};
use surco::doc::Block;
use surco::glyphs::Glyphs;
use surco::look::Look;
use surco::motion::Motion;
use surco::surface::Terminal;
use yunta_api::RunNews;
use yunta_core::Pid;
use yunta_replay::registry::Engine;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::Scene;

/// The frames a spinner turns through in Unicode, which nothing else a screen says draws.
const SPINNER: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";

/// A look that moves, in Unicode.
fn moving_look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        motion: Motion::Moves,
        ..Look::plain()
    }
}

/// `screen` on a look that moves.
fn moving(screen: &RunScreen) -> Screen {
    screen.state().screen(&moving_look())
}

/// Each row `screen` shows beside its pane and in it, on a look that moves.
fn moving_rows(screen: &RunScreen) -> Vec<String> {
    let look = moving_look();
    let shown = moving(screen);
    let terminal = Terminal::on(look);
    let side = shown.side.map(|side| terminal.lines(&side.region.doc));
    let main = terminal.lines(&shown.main.doc);
    side.into_iter()
        .flatten()
        .chain(main)
        .map(|line| line.text())
        .collect()
}

/// The keys `screen`'s legend names.
fn legend_keys(screen: &RunScreen) -> Vec<String> {
    let legend = screen.state().screen(&Look::plain()).legend;
    legend
        .0
        .into_iter()
        .filter_map(|binding| match binding.keys {
            Keys::Typed(typed) => Some(typed),
            _ => None,
        })
        .collect()
}

/// What the key `key` does as `screen`'s legend says it, where it names it.
fn legend_says(screen: &RunScreen, key: &str) -> Option<String> {
    let legend = screen.state().screen(&Look::plain()).legend;
    legend
        .0
        .into_iter()
        .find(|binding| matches!(&binding.keys, Keys::Typed(typed) if typed == key))
        .map(|binding| binding.does)
}

/// Whether `screen` stands as its node at work, its work or its loop's board.
fn at_work(screen: &RunScreen) -> bool {
    matches!(screen.state().scene(), Some(Scene::Work | Scene::Board))
}

/// A run held for a limit's reset goes on alone: its pane says when, and on a look that
/// moves no node and no task turns a spinner, as its engine is read again and again.
#[tokio::test]
async fn a_run_that_wakes_alone_turns_no_spinner_as_its_news_come() {
    let mut screen = run::opened("wakes-alone").await;
    assert!(at_work(&screen));
    for _ in 0..3 {
        let rows = moving_rows(&screen);
        let turning = rows
            .iter()
            .find(|row| row.chars().any(|c| SPINNER.contains(c)));
        assert_eq!(turning, None, "{rows:#?}");
        let when = rows
            .iter()
            .any(|row| row == "it goes on alone at 01:00 UTC");
        assert!(when, "{rows:#?}");
        screen.handed(RunNews::Engine(Engine::Unrecorded));
    }
}

/// A pause whose ask is not read yet shows its node at work, with no card in its pane and
/// none of the run's acts in its legend: what it asks stands there once read.
#[tokio::test]
async fn a_pause_not_read_yet_shows_its_work_and_no_card() {
    let screen = run::opened("still-unread").await;
    assert!(at_work(&screen));
    let main = moving(&screen).main;
    let card = main
        .doc
        .blocks()
        .iter()
        .any(|block| matches!(block, Block::Card(_)));
    assert!(!card, "no card stands before what it asks is read");
    assert_eq!(legend_keys(&screen), ["j k", "s", "g", "q"]);
}

/// A pause whose ask could not be read says why where its card stands, with no option to
/// choose, and no key answers it: the attention row stands empty, and `a` goes nowhere.
#[tokio::test]
async fn a_pause_whose_ask_could_not_be_read_says_why_in_the_cards_place() {
    let mut screen = run::opened("card-unread").await;
    assert!(at_work(&screen));
    let shown = moving(&screen);
    let Some(Block::Card(card)) = shown.main.doc.blocks().last() else {
        panic!(
            "the pane ends where its card stands: {:#?}",
            shown.main.doc.blocks().len()
        );
    };
    let why = card.about.join("\n");
    assert!(why.contains("1 error"), "{why}");
    assert!(shown.attention.is_none(), "its decision stands in sight");
    assert_eq!(legend_keys(&screen), ["j k", "s", "g", "q"]);
    let trail = shown.trail;
    screen.pressed(&[Stroke::Insert('a')]).await;
    assert_eq!(
        screen.state().screen(&Look::plain()).trail,
        trail,
        "`a` goes nowhere"
    );
}

/// An answer recorded with its resume not yet on the log says so above the work, and the
/// legend offers the resume while no engine takes it up; once one does, it offers none.
#[tokio::test]
async fn an_answer_awaiting_its_resume_is_offered_it_only_where_no_engine_drives_the_run() {
    let mut screen = run::opened("still-answered").await;
    assert!(at_work(&screen));
    assert_eq!(
        legend_says(&screen, "r").as_deref(),
        Some("continue it from its log")
    );
    let alive = Engine::Alive {
        pid: Pid::current(),
        prompt: None,
        words: None,
    };
    screen.handed(RunNews::Engine(alive));
    assert_eq!(legend_says(&screen, "r"), None, "an engine takes it up");
}
