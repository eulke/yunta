//! What the interface's own tests share beyond the world they work in
//! (`yunta_testkit_ui::sandbox`): the application open on a field the follower reads from
//! runs a bench made, each stopped under an id the test names, with the world it reads;
//! a performer working in a bench's world; a piece of work performed there; and a bench's
//! run read whole, with what it asks where a key is to answer it, as what the application
//! follows hands it first.
//!
//! Every test binary that declares `mod support` compiles the whole module and uses a
//! part of it, so what another binary needs reads as dead here.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use yunta_api::{FollowNews, Followed, RunNews, Stop};
use yunta_core::events::{Channel, EventPayload, PauseReason, RunEvent};
use yunta_core::RunId;
use yunta_testkit::workflows::FINISHES;
use yunta_testkit::{Bench, MOCK_CONFIG};
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::{Input, News, Operation, Outcome, Performer, Request, State};

/// A mock fixture with no sessions: the runs a field is made of open none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// How long a following takes at most to hand a bench's run read whole: its looks come a
/// tenth of a second apart, so only one that never reads the run waits this long.
const READING_WHOLE: Duration = Duration::from_secs(30);

/// The application open on the field of a repository set up whose runs are `runs`, each
/// created by a bench beside the others in one store and stopped as a person stops it, as
/// the follower reads them: opened with a performer in that world and driven until it
/// waits on nothing and stands caught up with the store's heads. The world comes back with
/// it: a test that works on in it keeps it, and one that lets it go leaves the state the
/// path of a store that is gone.
pub async fn on_the_field(runs: &[&RunId]) -> (State, Bench) {
    let world = Bench::new();
    let set_up = world.worktree.join(".yunta/config.yaml");
    let dir = set_up.parent().expect("the repository's .yunta");
    tokio::fs::create_dir_all(dir).await.expect("its .yunta");
    tokio::fs::write(&set_up, "defaults:\n  isolation: none\n")
        .await
        .expect("its config");
    for run in runs {
        stopped_beside(&world, run, FINISHES).await;
    }
    let (opened, asked) = State::opened(now());
    let (state, surfaced) = drive(opened, asked, &performer_in(&world), &[]).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    (state, world)
}

/// `run` created on `workflow` beside `world`'s runs, in its store, and stopped as a person
/// stops it: its bench, which wakes it again.
pub async fn stopped_beside(world: &Bench, run: &RunId, workflow: &str) -> Bench {
    let stopped = Stop::new();
    stopped.stop();
    let bench = world.beside(run.as_str());
    bench.create(workflow, NO_SESSIONS, MOCK_CONFIG).await;
    // Woken stopped, the run parks before its first node, as a stop between two
    // steps parks it.
    let bench = bench.with_cancel(stopped.work().clone());
    bench.wake().await;
    let logged = bench.events();
    let parked = logged.last().and_then(|event| event.payload());
    assert!(
        matches!(
            parked,
            Some(EventPayload::Run(RunEvent::Paused(paused)))
                if paused.pause == PauseReason::Cancelled
        ),
        "the run parks stopped: {parked:?}"
    );
    bench
}

/// A performer working in `bench`'s world, following the store its runs log in.
pub fn performer_in(bench: &Bench) -> Arc<Performer> {
    let root = bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    Arc::new(Performer::new(
        workspace(&bench.worktree, root),
        Channel::Tty,
    ))
}

/// What `operation` hands back, performed in `bench`'s world: what it finds while it
/// lasts, then what it comes to.
pub async fn performed(bench: &Bench, operation: Operation) -> (Vec<News>, Outcome) {
    // A ticket of the application's own: the performer hands back under what it is given.
    let (_, requests) = State::opened(now());
    let Some(Request::Perform(ticket, _)) = requests.into_iter().next() else {
        panic!("the application asks something as it opens");
    };
    let performer = performer_in(bench);
    let (inputs, mut received) = mpsc::unbounded_channel();
    performer.perform(ticket, operation, inputs).await;
    let mut found = Vec::new();
    loop {
        match received.recv().await {
            Some(Input::News(_, news)) => found.push(news),
            Some(Input::Outcome(_, outcome)) => return (found, outcome),
            Some(_) => continue,
            None => panic!("the performer hands back an outcome"),
        }
    }
}

/// `bench`'s run read whole, as a performer following it alone hands it first.
pub async fn read_whole(bench: &Bench) -> FollowNews {
    let run = Followed {
        field: None,
        run: Some(bench.run_id.clone()),
    };
    let (performer, (inputs, mut received)) = (performer_in(bench), mpsc::unbounded_channel());
    performer.take(Request::Follow(run), &inputs);
    loop {
        match tokio::time::timeout(READING_WHOLE, received.recv()).await {
            Ok(Some(Input::FollowNews(whole @ FollowNews::Whole { .. }))) => return whole,
            Ok(Some(Input::LookFailed(why))) => panic!("a look at the run failed: {why}"),
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => panic!("the following never reads {} whole", bench.run_id),
        }
    }
}

/// `bench`'s run read whole, then what it waits on a person for as the following read it
/// for the pause it stands at, as a performer following it alone hands them first: what a
/// state is handed before a key answers the run.
pub async fn read_whole_and_asked(bench: &Bench) -> Vec<FollowNews> {
    let run = Followed {
        field: None,
        run: Some(bench.run_id.clone()),
    };
    let (performer, (inputs, mut received)) = (performer_in(bench), mpsc::unbounded_channel());
    performer.take(Request::Follow(run), &inputs);
    let mut read = Vec::new();
    loop {
        match tokio::time::timeout(READING_WHOLE, received.recv()).await {
            Ok(Some(Input::FollowNews(whole @ FollowNews::Whole { .. }))) => read.push(whole),
            Ok(Some(Input::FollowNews(asked @ FollowNews::Run { .. }))) if asks(&asked) => {
                read.push(asked);
                return read;
            }
            Ok(Some(Input::LookFailed(why))) => panic!("a look at the run failed: {why}"),
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => panic!("the following never reads what {} asks", bench.run_id),
        }
    }
}

/// Whether `news` says what a run waits on a person for, read or not.
fn asks(news: &FollowNews) -> bool {
    matches!(
        news,
        FollowNews::Run {
            news: RunNews::Awaited(_) | RunNews::AwaitedUnread { .. },
            ..
        }
    )
}
