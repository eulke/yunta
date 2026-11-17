//! What the application follows, and what it keeps of it, through `State` alone: the
//! field in a repository set up and the run of a run's stage under every stage opened
//! over it, asked once each time it changes; what the performer's following hands,
//! folded once whatever is handed again, a promoted run's successor taking its place;
//! a look that failed, kept until the next news;
//! and the state caught up with the log once what it follows is read whole at the log's
//! heads, a run promoted before it is followed once its successor is folded. The news
//! comes from a performer following a bench's world.

#[path = "followed/promoted.rs"]
mod promoted;
mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use surco::look::Look;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use yunta_api::{Awaited, FieldReach, FollowNews, Followed, FollowedField, Repository, RunNews};
use yunta_core::events::{
    Channel, EventDraft, EventPayload, HostSuspendedPayload, RunEvent, RunPausedPayload,
    RunResumedPayload,
};
use yunta_core::{RunId, Seq};
use yunta_testkit::workflows::{FINISHES, GATED, PLANNER_HANDS_TWO_TASKS, PLAN_THEN_GATE};
use yunta_testkit::Bench;
use yunta_testkit_core::FixedClock;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::prompt::Stroke;
use yunta_ui::{
    AwaitedReading, Input, Outcome, Performer, RepositoryReading, Request, RunInView, State,
};

use support::{on_the_field, read_whole};

/// A mock fixture with no sessions: none of the workflows opens one.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// How long a following takes at most to hand a bench's world whole: its looks come a
/// tenth of a second apart and what a run waits on is read on a task of its own, so only
/// a state that never catches up waits thirty seconds.
const CATCHING_UP: Duration = Duration::from_secs(30);

/// A run stopped in the field a test stands on.
const RUN: RunId = RunId::from_static("01M7F0110WEDSTANDS00000000");

/// What a surface does for a state, with a performer working in a bench's world: it
/// hands the performer what the state asks to follow, and the state what the following
/// reads.
struct Surface {
    performer: Arc<Performer>,
    inputs: UnboundedSender<Input>,
    received: UnboundedReceiver<Input>,
}

impl Surface {
    /// A surface whose performer works in `bench`'s world.
    fn over(bench: &Bench) -> Surface {
        let store = bench.store_path();
        let root = store.parent().expect("a bench's log stands in its root");
        let performer = Arc::new(Performer::new(
            workspace(&bench.worktree, root),
            Channel::Tty,
        ));
        let (inputs, received) = mpsc::unbounded_channel();
        Surface {
            performer,
            inputs,
            received,
        }
    }

    /// Each request to follow among `asked` handed to the performer, then what its
    /// following hands taken by `state` until it stands caught up with the log of
    /// `bench`: each piece, in the order it came.
    async fn caught_up(
        &mut self,
        state: &mut State,
        mut asked: Vec<Request>,
        bench: &Bench,
    ) -> Vec<Input> {
        let heads = yunta_testkit::heads(bench.store_path());
        let mut taken = Vec::new();
        loop {
            for request in asked.drain(..) {
                if matches!(request, Request::Follow(_)) {
                    self.performer.take(request, &self.inputs);
                }
            }
            if state.caught_up(&heads) {
                return taken;
            }
            let came = tokio::time::timeout(CATCHING_UP, self.received.recv());
            let input = came
                .await
                .expect("the state catches up with the log")
                .expect("the following hands what it reads");
            asked = state.update(input.clone());
            taken.push(input);
        }
    }

    /// What the following hands of `run` up to the next reading of what it waits on: the
    /// events in one batch, as one look past them would hand them, and that reading.
    async fn up_to_the_next_reading(&mut self, run: &RunId) -> (Input, Input) {
        let mut batch = Vec::new();
        loop {
            let came = tokio::time::timeout(CATCHING_UP, self.received.recv());
            let input = came
                .await
                .expect("the run is read again")
                .expect("the following hands what it reads");
            let Input::FollowNews(FollowNews::Run { news, .. }) = &input else {
                continue;
            };
            match news {
                RunNews::Events(events) => batch.extend(events.iter().cloned()),
                RunNews::Awaited(Some(_)) => {
                    let news = RunNews::Events(batch);
                    let run = run.clone();
                    return (Input::FollowNews(FollowNews::Run { run, news }), input);
                }
                _ => {}
            }
        }
    }
}

/// `payload` appended to `bench`'s run behind the engine's back: the seq it took.
fn appended(bench: &Bench, payload: EventPayload) -> Seq {
    let draft = EventDraft::new(bench.run_id.clone(), None, payload).unwrap();
    bench.storage.append(&draft, &FixedClock).unwrap()
}

/// An event that moves nothing but the clock of `bench`'s run: the seq it took.
fn slept(bench: &Bench) -> Seq {
    let slept = HostSuspendedPayload::slept(Duration::from_secs(3));
    appended(bench, EventPayload::Run(RunEvent::HostSuspended(slept)))
}

/// `bench`'s parked run resumed, then parked again on the pause it stood at: a pause of
/// its own.
fn parked_again(bench: &Bench) {
    let logged = bench.storage.events_for_run(&bench.run_id).unwrap();
    let pause = logged.iter().rev().find_map(|event| match event.payload() {
        Some(EventPayload::Run(RunEvent::Paused(paused))) => Some(paused.pause.clone()),
        _ => None,
    });
    let pause = pause.expect("the run parked");
    let resumed = RunResumedPayload::new(Vec::new(), None);
    appended(bench, EventPayload::Run(RunEvent::Resumed(resumed)));
    appended(
        bench,
        EventPayload::Run(RunEvent::Paused(RunPausedPayload { pause })),
    );
}

/// The application on a field listing `run`, with its stage opened over the field, and
/// what opening it asked.
async fn on_the_run(run: &RunId) -> (State, Vec<Request>) {
    let (mut state, _) = on_the_field(&[run]).await;
    let asked = state.update(Input::Stroke(Stroke::Enter));
    (state, asked)
}

/// The run `state` keeps in view.
fn in_view(state: &State) -> &RunInView {
    let in_view = state.followed_runs().in_view.as_ref();
    in_view.expect("the run in view is kept")
}

/// The requests to follow among `asked`.
fn follows(asked: &[Request]) -> Vec<&Followed> {
    asked
        .iter()
        .filter_map(|request| match request {
            Request::Follow(followed) => Some(followed),
            _ => None,
        })
        .collect()
}

/// The application opened, and what it asks once the repository it works in reads as
/// `repository`.
fn opened_in(repository: Repository) -> (State, Vec<Request>) {
    let (mut state, asked) = State::opened(now());
    let [Request::Perform(opening, _)] = asked.as_slice() else {
        panic!("the application asks where it stands as it opens: {asked:?}");
    };
    let read = Outcome::Repository(Box::new(RepositoryReading {
        repository,
        storage: PathBuf::from("/home/me/.yunta/yunta.db"),
        prices: None,
    }));
    let asked = state.update(Input::Outcome(*opening, read));
    (state, asked)
}

/// A look that failed is kept with its why, so what was read before reads as old as the
/// last look that read; the next news of the field clears it.
#[tokio::test]
async fn a_look_that_failed_is_kept_until_the_next_news() {
    let (mut state, _) = on_the_field(&[&RUN]).await;
    let failed = "failed to list runs".to_string();
    state.update(Input::LookFailed(failed.clone()));
    assert_eq!(state.followed_runs().look_failed, Some(failed));

    let field = FollowedField {
        runs: Vec::new(),
        unreadable: Vec::new(),
        elsewhere: 0,
        here: true,
    };
    state.update(Input::FollowNews(FollowNews::Field(Box::new(field))));
    assert_eq!(state.followed_runs().look_failed, None);
}

/// In a repository set up, the repository read, the field here is followed before the
/// application stands anywhere; outside a repository nothing is.
#[test]
fn the_field_here_is_followed_before_the_application_stands() {
    let here = Followed {
        field: Some(FieldReach::Here),
        run: None,
    };
    let (state, asked) = opened_in(Repository::SetUp);
    assert_eq!(follows(&asked), [&here]);
    assert_eq!(state.followed(), here);

    let (state, asked) = opened_in(Repository::Absent);
    assert_eq!(follows(&asked), Vec::<&Followed>::new());
    let nothing = Followed {
        field: None,
        run: None,
    };
    assert_eq!(state.followed(), nothing, "the signpost follows nothing");
}

/// A run's stage opened from the field follows the run beside the field; its stats
/// opened over it, and help and the palette over those, ask to follow nothing more, the
/// run followed under each; going back to the field follows the field alone.
#[tokio::test]
async fn the_run_is_followed_under_every_stage_opened_over_it() {
    let bench = Bench::with_run_id("01M7F0110WEDSTAGES00000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let run = bench.run_id.clone();
    let (mut state, asked) = on_the_run(&run).await;
    let with_run = Followed {
        field: Some(FieldReach::Here),
        run: Some(run.clone()),
    };
    assert_eq!(follows(&asked), [&with_run]);

    // The run's stage offers its stats once the run is read, handed as what the state
    // follows hands it.
    state.update(Input::FollowNews(read_whole(&bench).await));
    for stroke in ['s', '?', '?', '/'].map(Stroke::Insert) {
        let asked = state.update(Input::Stroke(stroke));
        assert_eq!(follows(&asked), Vec::<&Followed>::new(), "{stroke:?}");
        assert_eq!(state.followed(), with_run, "under {stroke:?}");
    }
    for _ in 0..2 {
        let asked = state.update(Input::Stroke(Stroke::Decline));
        assert_eq!(follows(&asked), Vec::<&Followed>::new());
    }

    let asked = state.update(Input::Stroke(Stroke::Decline));
    let field = Followed {
        field: Some(FieldReach::Here),
        run: None,
    };
    assert_eq!(follows(&asked), [&field]);
}

/// A run that finished and one parked on a gate beside it, in one log.
async fn finished_and_parked() -> (Bench, Bench) {
    let finished = Bench::with_run_id("01M7F0110WEDF1N1SHED000000");
    finished.run(FINISHES, NO_SESSIONS).await;
    let parked = finished.beside("01M7F0110WEDPARKED00000000");
    parked.run(GATED, NO_SESSIONS).await;
    (finished, parked)
}

/// Every piece of news a bench's world hands, a batch of events among them, handed again
/// changes nothing: the state fed it all twice is the state fed it once.
#[tokio::test]
async fn news_handed_again_changes_nothing() {
    let (finished, parked) = finished_and_parked().await;
    let mut surface = Surface::over(&finished);
    let (mut once, asked) = on_the_run(&parked.run_id).await;
    // Fed from the same opening: each opening reads a field of a store of its own.
    let mut twice = once.clone();
    let mut news = surface.caught_up(&mut once, asked, &finished).await;
    slept(&parked);
    news.extend(surface.caught_up(&mut once, Vec::new(), &finished).await);
    let batch = |input: &Input| {
        let events = |news: &RunNews| matches!(news, RunNews::Events(_));
        matches!(input, Input::FollowNews(FollowNews::Run { news, .. }) if events(news))
    };
    assert!(news.iter().any(batch), "a batch of events comes: {news:?}");

    for input in news.iter().chain(&news) {
        twice.update(input.clone());
    }
    assert_eq!(format!("{twice:?}"), format!("{once:?}"));
}

/// The state stands caught up once the field and the run are read whole at the log's
/// heads, the decision the run waits on read; never before, nor at heads past the log's.
#[tokio::test]
async fn caught_up_once_what_is_followed_is_read_whole() {
    let (finished, parked) = finished_and_parked().await;
    let mut surface = Surface::over(&finished);
    let (mut state, asked) = on_the_run(&parked.run_id).await;
    let at_the_log = yunta_testkit::heads(finished.store_path());
    assert!(!state.caught_up(&at_the_log), "nothing is read yet");

    let news = surface.caught_up(&mut state, asked, &finished).await;
    let came = |whole: fn(&FollowNews) -> bool| {
        news.iter()
            .any(|input| matches!(input, Input::FollowNews(piece) if whole(piece)))
    };
    assert!(
        came(|piece| matches!(piece, FollowNews::Field(_))),
        "the field"
    );
    assert!(
        came(|piece| matches!(piece, FollowNews::Whole { .. })),
        "the run"
    );
    let awaited = &in_view(&state).awaited;
    assert!(
        matches!(
            awaited,
            Some(AwaitedReading::Read {
                awaited: Awaited::Decision(_),
                ..
            })
        ),
        "the decision the run waits on is read: {awaited:?}"
    );

    let mut past = at_the_log.clone();
    past.insert(parked.run_id.clone(), at_the_log[&parked.run_id].next());
    assert!(!state.caught_up(&past), "a head the run has not reached");
}

/// A promoted run's successor, folded from what is followed, takes the run's place: the
/// run's stage shows the successor, which the state follows and keeps in view, and the
/// screen's trail ends with its handle.
#[tokio::test]
async fn a_successor_folded_takes_the_place_of_its_run() {
    let promoted = Bench::with_run_id("01M7F0110WEDPR0M0TED000000");
    promoted.run(FINISHES, NO_SESSIONS).await;
    let successor = promoted.beside("01M7F0110WEDNEXT0000000000");
    successor.run(FINISHES, NO_SESSIONS).await;
    let (mut state, _) = on_the_run(&promoted.run_id).await;
    state.update(Input::FollowNews(read_whole(&promoted).await));

    let FollowNews::Whole { run, events } = read_whole(&successor).await else {
        unreachable!("a run read whole");
    };
    let handed_on = FollowNews::Run {
        run: promoted.run_id.clone(),
        news: RunNews::Successor { run, events },
    };
    let asked = state.update(Input::FollowNews(handed_on));
    let onto = Followed {
        field: Some(FieldReach::Here),
        run: Some(successor.run_id.clone()),
    };
    assert_eq!(follows(&asked), [&onto], "the successor is followed");
    assert_eq!(state.followed(), onto);
    assert_eq!(in_view(&state).run.run, successor.run_id);
    let trail = state.screen(&Look::plain()).trail;
    assert_eq!(
        trail.last().map(String::as_str),
        Some(successor.run_id.handle()),
        "{trail:?}"
    );
}

/// Every object of `bench`'s run rewritten as bytes no document reads as: each document
/// its nodes handed over fails its shape.
fn broken_in_its_objects(bench: &Bench) {
    let objects = bench.run_dir().join(yunta_core::run_dir::OBJECTS_DIR);
    for object in std::fs::read_dir(objects).expect("the run's objects") {
        let object = object.expect("an object of the run").path();
        std::fs::write(object, b"tasks: 7\n").expect("the object rewritten");
    }
}

/// A run parked on a decision whose shown document is broken in its objects: what it
/// waits on is kept unread with why, and the next news of the run still folds.
#[tokio::test]
async fn what_a_run_waits_on_is_kept_unread_with_why() {
    let bench = Bench::with_run_id("01M7F0110WEDBR0KENSHAPE000");
    bench.run(PLAN_THEN_GATE, PLANNER_HANDS_TWO_TASKS).await;
    broken_in_its_objects(&bench);
    let mut surface = Surface::over(&bench);
    let (mut state, asked) = on_the_run(&bench.run_id).await;
    surface.caught_up(&mut state, asked, &bench).await;
    let unread = |state: &State| match &in_view(state).awaited {
        Some(AwaitedReading::Unread { why, .. }) => why.clone(),
        other => panic!("what the run waits on is kept unread: {other:?}"),
    };
    let why = unread(&state);
    assert!(!why.is_empty(), "the why is kept");

    let seq = slept(&bench);
    surface.caught_up(&mut state, Vec::new(), &bench).await;
    assert_eq!(
        in_view(&state).run.state.head(),
        Some(seq),
        "the news folds"
    );
    assert_eq!(unread(&state), why, "what it waits on stays unread");
}

/// A run that goes from one pause to the next with no look between stands caught up only
/// once the second pause's reading folds: the first pause's reading is for a pause it no
/// longer stands at.
#[tokio::test]
async fn a_reading_is_held_to_the_pause_the_run_stands_at() {
    let bench = Bench::with_run_id("01M7F0110WEDPARKEDAGA1N000");
    bench.run(GATED, NO_SESSIONS).await;
    let mut surface = Surface::over(&bench);
    let (mut state, asked) = on_the_run(&bench.run_id).await;
    surface.caught_up(&mut state, asked, &bench).await;

    parked_again(&bench);
    let (batch, reading) = surface.up_to_the_next_reading(&bench.run_id).await;
    state.update(batch);
    let at_the_log = yunta_testkit::heads(bench.store_path());
    assert!(
        !state.caught_up(&at_the_log),
        "the reading kept is for the pause the run left"
    );
    state.update(reading);
    assert!(state.caught_up(&at_the_log), "the second pause read");
}
