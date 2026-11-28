//! Leaving a run's screen, through `State` alone, on the cuts of the targets of a run's
//! screen: `q` and `esc` go down to the field, which notes until the next key whether the
//! run keeps going, waits or ended, in render's sentence for each; the field is the one
//! the state keeps, and nothing of the run is left over it.

use surco::surface::{Markdown, Surface};
use yunta_api::{FollowNews, RunNews};
use yunta_core::events::{
    EventBody, EventPayload, RunEvent, RunFinishedPayload, RunMetrics, StoredEvent, TerminalState,
    TokenUsage,
};
use yunta_render::screen::Leaving;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Request, State};

use super::key_does;

/// `key` pressed on a copy of `screen`'s state, which stands as the key leaves it, and what
/// the key asked.
fn left_by(screen: &RunScreen, key: Stroke) -> (State, Vec<Request>) {
    let mut state = screen.state().clone();
    let asked = state.update(Input::Stroke(key));
    (state, asked)
}

/// What `state`'s main region says, as Markdown draws it.
fn main_region(state: &State) -> String {
    Markdown.draw(&state.screen(&Markdown.look()).main.doc)
}

/// What `state` notes in its attention row's place, where it notes anything.
fn noted(state: &State) -> Option<String> {
    let noted = state.screen(&Markdown.look()).noted;
    noted.map(|noted| noted.said)
}

/// `state` stands on the field of the repository it opened in, following no run, the note
/// `leaving` says in its attention row's place.
fn on_the_field_saying(state: &State, asked: &[Request], leaving: Leaving) {
    assert!(!asked.contains(&Request::Leave), "it stays open: {asked:?}");
    let screen = state.screen(&Markdown.look());
    let repository = state
        .followed_runs()
        .repository
        .as_ref()
        .and_then(yunta_api::Repository::dir);
    let place = repository.map(|place| place.display().to_string());
    assert_eq!(screen.trail.get(1), place.as_ref(), "on the field");
    assert_eq!(state.followed().run, None, "no run is followed under it");
    assert_eq!(noted(state).as_deref(), Some(leaving.said()));
}

#[tokio::test]
async fn q_on_a_run_at_work_goes_to_the_field_saying_it_keeps_going() {
    let work = run::opened("work").await;
    assert_eq!(
        key_does(work.state(), "q").as_deref(),
        Some("field, it keeps going")
    );
    let (mut field, asked) = left_by(&work, Stroke::Insert('q'));
    on_the_field_saying(&field, &asked, Leaving::KeepsGoing);
    assert!(
        main_region(&field).contains("Q78GRR"),
        "the field the state keeps, the run among it"
    );
    field.update(Input::Stroke(Stroke::Insert('j')));
    assert_eq!(noted(&field), None, "the next key takes the note away");
}

/// A run no engine drives waits, though it stands created or running, as one created and
/// never started and one whose answer awaits a resume nobody runs stand: the legend's `q`
/// says only the field, and `q` leaves the waiting note.
#[tokio::test]
async fn q_on_a_run_no_engine_drives_says_it_waits_though_it_stands_created_or_running() {
    for target in ["created", "still-answered"] {
        let screen = run::opened(target).await;
        assert_eq!(
            key_does(screen.state(), "q").as_deref(),
            Some("field"),
            "`{target}`"
        );
        let (field, asked) = left_by(&screen, Stroke::Insert('q'));
        on_the_field_saying(&field, &asked, Leaving::Waits);
    }
}

/// A run that closed cancelled has ended, though its headline calls it stopped as it does a
/// run a person paused: `q` says so, and its legend says only the field. The close comes as
/// its following hands it, to a copy of the run's screen.
#[tokio::test]
async fn q_on_a_run_that_closed_cancelled_says_it_ended() {
    let stopped = run::opened("stopped").await;
    let mut state = stopped.state().clone();
    let in_view = state.followed_runs().in_view.clone();
    let in_view = in_view.expect("the run is in view");
    let head = in_view.run.state().head().expect("a run with events");
    let at = in_view
        .run
        .state()
        .clock()
        .last_at()
        .expect("a run with events");
    let closed = RunFinishedPayload::closed(
        TerminalState::Cancelled,
        RunMetrics::of(TokenUsage::default(), 0),
    );
    let closing = StoredEvent {
        run_id: in_view.run.run().clone(),
        seq: (head.get() + 1).into(),
        timestamp: at,
        node_id: None,
        body: EventBody::Known(EventPayload::Run(RunEvent::Finished(closed))),
    };
    let run = in_view.run.run().clone();
    let news = RunNews::Events(vec![closing]);
    state.update(Input::FollowNews(FollowNews::Run { run, news }));
    assert_eq!(key_does(&state, "q").as_deref(), Some("field"));
    let asked = state.update(Input::Stroke(Stroke::Insert('q')));
    on_the_field_saying(&state, &asked, Leaving::Ended);
}

/// An answer recorded in place hands the run to a detached engine, which writes its registry
/// once it holds the run: until a reading names it, the run keeps going, so neither the
/// legend nor `q` says it waits, and no `r` offers to take it up a second time. The cut's
/// engine writes no registry, as one that has not yet done so.
#[tokio::test]
async fn q_right_after_an_answer_recorded_in_place_says_the_run_keeps_going() {
    let mut scope = run::opened("scope").await;
    scope.pressed(&[Stroke::Enter]).await;
    assert_eq!(
        key_does(scope.state(), "q").as_deref(),
        Some("field, it keeps going")
    );
    assert_eq!(key_does(scope.state(), "r"), None, "nothing to take up");
    let (field, asked) = left_by(&scope, Stroke::Insert('q'));
    on_the_field_saying(&field, &asked, Leaving::KeepsGoing);
}

#[tokio::test]
async fn q_on_a_run_waiting_on_the_person_goes_to_the_field_saying_it_waits() {
    let failure = run::opened("failure").await;
    let (field, asked) = left_by(&failure, Stroke::Insert('q'));
    on_the_field_saying(&field, &asked, Leaving::Waits);
}

#[tokio::test]
async fn q_on_a_run_that_ended_goes_to_the_field_saying_so() {
    let receipt = run::opened("receipt").await;
    let (field, asked) = left_by(&receipt, Stroke::Insert('q'));
    on_the_field_saying(&field, &asked, Leaving::Ended);
}

#[tokio::test]
async fn esc_on_a_run_goes_to_the_field_as_q_does() {
    let stopped = run::opened("stopped").await;
    let (field, asked) = left_by(&stopped, Stroke::Decline);
    on_the_field_saying(&field, &asked, Leaving::Waits);
    // A node the person points at, its own scene shown, is left with the run.
    let mut work = run::opened("work").await;
    work.pressed(&[Stroke::Insert('j')]).await;
    let (field, asked) = left_by(&work, Stroke::Decline);
    on_the_field_saying(&field, &asked, Leaving::KeepsGoing);
}
