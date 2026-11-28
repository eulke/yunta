//! A run waiting on its pull request, through `State` alone, in what its frames cannot hold:
//! its forge is read once as the scene opens, and not again on news of the same pause, nor as
//! the person comes back from a stage opened over the run before the forge answered, the
//! answer landing on the run under it;
//! `o` asks the surface to show the pull request by its link, where on every other scene it
//! shows the run's page; and a link the surface could not show is said, with the link.

use surco::glyphs::Mark;
use surco::look::Look;
use yunta_api::{FollowNews, FollowedField, RunNews};
use yunta_core::step::NextStep;
use yunta_replay::registry::Engine;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Address, Input, Operation, Outcome, RepositoryReading, Request, State, WebLink};

use super::scene_of;

/// The link of the pull request the small run's gate published.
const LINK: &str = "https://github.com/acme/web/pull/7";

/// The pull request the small run's gate published, as a page a surface shows.
fn pull_request() -> WebLink {
    WebLink::https(LINK).expect("an https link")
}

#[tokio::test]
async fn the_forge_is_read_once_as_the_scene_opens_and_not_again_on_news_of_the_same_pause() {
    let mut screen = run::opened("pull-request").await;
    assert_eq!(scene_of(&screen), Some("pull request"));
    assert_eq!(
        screen.forge_polls(),
        1,
        "the forge is read as the scene opens"
    );
    // The run's registry read again, the run standing at the same pause: news that asks
    // for nothing, and drives nothing after it.
    screen.handed(RunNews::Engine(Engine::Unrecorded));
    screen.pressed(&[]).await;
    assert_eq!(
        screen.forge_polls(),
        1,
        "news of the same pause reads the forge no more"
    );
}

#[tokio::test]
async fn o_asks_the_surface_to_show_the_pull_request_by_its_link() {
    let mut screen = run::opened("pull-request").await;
    let surfaced = screen.pressed(&[Stroke::Insert('o')]).await;
    assert_eq!(surfaced, [Request::ShowPage(Address::Link(pull_request()))]);
}

/// A link the surface could not open is a failed act: the screen notes it under the failed
/// mark, in the words a page that could not be shown is said in, with the link to take.
#[tokio::test]
async fn a_link_that_could_not_be_opened_is_noted_with_it_as_a_failed_act() {
    let mut screen = run::opened("pull-request").await;
    let link = Address::Link(pull_request());
    let surfaced = screen.pressed(&[Stroke::Insert('o')]).await;
    assert_eq!(surfaced, [Request::ShowPage(link.clone())]);
    screen.unshown(link);
    let noted = screen.state().screen(&Look::plain()).noted;
    let noted = noted.expect("the screen notes what it could not open");
    assert_eq!(noted.mark, Mark::Failed);
    assert_eq!(
        noted.said,
        "nothing here could open it: open https://github.com/acme/web/pull/7 in a browser."
    );
}

/// The tickets of the readings of a pull request `asked` asks for.
fn polls(asked: &[Request]) -> Vec<yunta_ui::Ticket> {
    let polled = |request: &Request| match request {
        Request::Perform(ticket, Operation::ReadPullRequest { .. }) => Some(*ticket),
        _ => None,
    };
    asked.iter().filter_map(polled).collect()
}

/// A state opened on the run `screen` shows, handed what `screen`'s state read of the
/// repository, of the field and of the run, its work left unanswered: what the run's stage
/// asks once it reads the run, with the state that asks it.
fn opened_unanswered(screen: &RunScreen) -> (State, Vec<Request>) {
    let kept = screen.state().followed_runs();
    let in_view = kept.in_view.as_deref().expect("the run is in view");
    let field = kept.field.as_ref().expect("the field is read");
    let now = in_view
        .run
        .state()
        .clock()
        .last_at()
        .expect("the run has events");
    let step = NextStep::status(in_view.run.run());
    let (mut state, asked) = State::opened_on(now, &step).expect("a run's step opens it");
    let [Request::Perform(repository, _)] = asked.as_slice() else {
        panic!("the repository is read first: {asked:?}");
    };
    let read = RepositoryReading {
        repository: kept.repository.clone().expect("the repository is read"),
        storage: kept.storage.clone().expect("the store is read"),
        prices: None,
        home: None,
    };
    state.update(Input::Outcome(
        *repository,
        Outcome::Repository(Box::new(read)),
    ));
    let whole = FollowedField {
        runs: field.runs.values().map(|run| (**run).clone()).collect(),
        unreadable: field.unreadable.clone(),
        elsewhere: field.elsewhere,
        here: field.here,
    };
    state.update(Input::FollowNews(FollowNews::Field(Box::new(whole))));
    let run = FollowNews::Whole {
        run: Box::new(in_view.run.clone()),
        events: Vec::new(),
    };
    let asked = state.update(Input::FollowNews(run));
    (state, asked)
}

/// A reading of the pull request under way as a stage opens over the run stays the run's:
/// the stage over it lets nothing go, the forge's answer lands on the run under it, and coming
/// back asks nothing again.
#[tokio::test]
async fn a_poll_under_way_as_a_stage_opens_over_the_run_is_answered_there() {
    let screen = run::opened("pull-request").await;
    let (mut state, asked) = opened_unanswered(&screen);
    let [polled] = polls(&asked)[..] else {
        panic!("the forge is read as the scene opens: {asked:?}");
    };
    let waits_on_it = |state: &State| state.waiting_on().iter().any(|(on, _)| *on == polled);
    let over = state.update(Input::Stroke(Stroke::Insert('s')));
    assert!(
        !over.contains(&Request::LetGo(polled)) && waits_on_it(&state),
        "a stage opened over the run lets the reading go on: {over:?}"
    );
    let answered = state.update(Input::Outcome(polled, Outcome::PullRequest(None)));
    assert!(answered.is_empty(), "{answered:?}");
    assert!(!waits_on_it(&state));
    let back = state.update(Input::Stroke(Stroke::Decline));
    assert!(
        polls(&back).is_empty(),
        "what the forge answered stands on the run: {back:?}"
    );
}
