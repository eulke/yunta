//! An answer given elsewhere to what a run's screen holds in place, through `State` alone, on
//! the design's run: a decision recorded through the API, as `yunta resolve-gate` at a shell
//! records it, takes the card away even under a person typing on it, the note saying it is
//! answered and by whom in the log's words, and what was typed goes with the pause; a round's
//! answers recorded the same way take the round away alike, whatever was picked on it. A
//! pause is read against its own run's log alone: a prompt let go as the screen moves onto
//! another run says nothing of what that run's log holds.

use surco::glyphs::Mark;
use surco::look::Look;
use yunta_api::{FollowNews, RunNews};
use yunta_core::RunId;
use yunta_storage::Storage;
use yunta_testkit_ui::screens::run;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Scene};

use super::{card, decided};

#[tokio::test]
async fn a_decision_answered_elsewhere_takes_the_card_away_under_a_person_typing_and_says_who() {
    let mut screen = run::opened("scope").await;
    let typing: Vec<Stroke> = "only for the audit".chars().map(Stroke::Insert).collect();
    screen.pressed(&[Stroke::Insert('n')]).await;
    screen.pressed(&typing).await;
    assert!(
        screen.state().screen(&Look::plain()).typed,
        "a line is typed on the card"
    );
    let before = decided(&screen);

    screen.decided_elsewhere("deny", "lead").await;
    assert_eq!(decided(&screen)[before.len()..], ["deny"], "answered once");
    assert!(card(&screen).is_none(), "the card goes with its pause");
    assert!(
        !matches!(screen.state().scene(), Some(Scene::Card(..))),
        "{:?}",
        screen.state().scene()
    );
    let drawn = screen.state().screen(&Look::plain());
    assert!(!drawn.typed, "what was typed goes with the pause");
    let noted = drawn.noted.expect("the answer given elsewhere is noted");
    assert_eq!(noted.mark, Mark::Done, "{}", noted.said);
    assert_eq!(noted.said, "Answered elsewhere: `deny` chosen by lead.");
}

#[tokio::test]
async fn a_round_answered_elsewhere_takes_the_round_away_and_says_who() {
    let mut screen = run::opened("round").await;
    screen.pressed(&[Stroke::Down]).await;
    assert!(matches!(screen.state().scene(), Some(Scene::Round)));

    let answers = [
        (
            "where-global-lives",
            "The current user's platform data directory, shared by that user's projects",
        ),
        (
            "how-requested",
            "--global on pack add, update, remove and list",
        ),
    ];
    screen.answered_elsewhere("grill", &answers, "lead").await;
    assert!(
        !matches!(screen.state().scene(), Some(Scene::Round)),
        "the round goes with its pause: {:?}",
        screen.state().scene()
    );
    let drawn = screen.state().screen(&Look::plain());
    assert!(!drawn.typed, "what was picked goes with the pause");
    let noted = drawn.noted.expect("the answers given elsewhere are noted");
    assert_eq!(noted.mark, Mark::Done, "{}", noted.said);
    assert_eq!(noted.said, "Answered elsewhere: by lead via mcp.");
}

/// The run a promotion hands the design's run to, in this test: what the screen moves onto
/// while the run leads its round.
const SUCCESSOR: &str = "01M7DES1GNED0000SVCCESS0R0";

#[tokio::test]
async fn a_round_let_go_as_the_screen_moves_onto_another_run_says_nothing_of_that_runs_log() {
    let screen = run::opened("round").await;
    assert!(matches!(screen.state().scene(), Some(Scene::Round)));
    // Another run whose log holds `grill`'s round answered after the pause the round in place
    // belongs to: the design's run once it went on past its round, under another id.
    let went_on = run::opened("work").await;
    let in_view = went_on.state().followed_runs().in_view.clone();
    let went_on_run = in_view.expect("the run in view").run.clone();
    let storage = Storage::open(&went_on.store()).expect("the cut's log");
    let events = storage.events_for_run(went_on_run.run()).expect("its log");
    let successor: RunId = SUCCESSOR.parse().expect("a run's id");
    let (manifest, engine) = (went_on_run.manifest(), went_on_run.engine());
    let another =
        yunta_api::FollowedRun::read(successor.clone(), manifest.clone(), &events, engine.clone());

    let mut state = screen.state().clone();
    let run = state
        .followed()
        .run
        .expect("a run's screen follows its run");
    let news = RunNews::Successor {
        run: Box::new(another),
        events,
    };
    state.update(Input::FollowNews(FollowNews::Run { run, news }));
    assert_eq!(state.followed().run, Some(successor), "the screen moved on");
    assert!(
        !matches!(state.scene(), Some(Scene::Round)),
        "the round goes with its run: {:?}",
        state.scene()
    );
    let noted = state.screen(&Look::plain()).noted;
    assert!(
        noted.is_none(),
        "another run's log answers no pause of this one: {:?}",
        noted.map(|noted| noted.said)
    );
}
