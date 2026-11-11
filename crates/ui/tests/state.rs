//! The application as values, through `State` alone: what it asks as it opens, an
//! answer that comes late, and where it stands outside a repository.

use chrono::{DateTime, Utc};
use surco::look::Look;
use surco::surface::{Markdown, Surface};
use yunta_api::Repository;
use yunta_render::page::{Page, Signpost};
use yunta_ui::{Input, Operation, Outcome, Request, State};

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000, 0).expect("an instant")
}

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

/// Opening, the application asks where it stands before it shows anything.
#[test]
fn opening_asks_where_it_stands() {
    let (state, requests) = State::opened(now());
    let [Request::Perform(_, Operation::ReadRepository)] = requests.as_slice() else {
        panic!("opening asks whether it stands in a repository: {requests:?}");
    };
    assert_eq!(
        main_region(&state),
        "",
        "nothing stands on screen until it is answered"
    );
}

/// Outside a repository the screen is the signpost, and its trail starts at yunta.
#[test]
fn outside_a_repository_the_screen_is_the_signpost() {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    let asked = state.update(Input::Outcome(
        *ticket,
        Outcome::Repository(Repository::Absent),
    ));
    assert!(asked.is_empty(), "{asked:?}");
    let look = Markdown.look();
    assert_eq!(
        main_region(&state),
        Markdown.draw(&Signpost.document(&look))
    );
    assert_eq!(state.screen(&Look::plain()).trail, ["yunta"]);
}

/// An answer to a ticket the state no longer waits on changes nothing: a stage left
/// can never be undone by a late answer.
#[test]
fn a_late_outcome_is_dropped() {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    state.update(Input::Outcome(
        *ticket,
        Outcome::Repository(Repository::NeverSetUp),
    ));
    let before = main_region(&state);
    // Taken, this answer would put the signpost on screen.
    let asked = state.update(Input::Outcome(
        *ticket,
        Outcome::Repository(Repository::Absent),
    ));
    assert!(asked.is_empty(), "{asked:?}");
    assert_eq!(main_region(&state), before);
}

/// Where the application could not learn where it stands, its screen says what
/// stopped it, and leaving is what is left.
#[test]
fn a_failure_to_learn_where_it_stands_is_said() {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    let failed = Outcome::Failed("git is not on PATH".to_string());
    state.update(Input::Outcome(*ticket, failed));
    assert!(
        main_region(&state).contains("git is not on PATH"),
        "{}",
        main_region(&state)
    );
    let left = state.update(Input::Stroke(yunta_ui::prompt::Stroke::Insert('q')));
    assert_eq!(left, [Request::Leave]);
}

/// The state is a value: a clone goes its own way.
#[test]
fn a_clone_goes_its_own_way() {
    let (state, requests) = State::opened(now());
    let [Request::Perform(ticket, _)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    let mut answered = state.clone();
    answered.update(Input::Outcome(
        *ticket,
        Outcome::Repository(Repository::Absent),
    ));
    assert_eq!(main_region(&state), "");
    assert_ne!(main_region(&answered), "");
}
