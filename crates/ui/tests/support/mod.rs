//! What the tests of the application share: a workspace made in a test's own
//! directory, which reads nothing of the machine's, a state driven with a performer
//! until it waits on nothing, a piece of work performed in a bench's world, and the
//! application open on a field of runs a test makes up.
//!
//! Every test binary that declares `mod support` compiles the whole module and uses a
//! part of it, so what another binary needs reads as dead here.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use surco::words::Standing;
use tokio::sync::mpsc;
use yunta_api::{FieldReach, FieldReading, Repository, Workspace};
use yunta_core::events::PauseReason;
use yunta_core::{Env, ProcessSecrets, RunId, SystemIdSource};
use yunta_render::page::FieldRow;
use yunta_replay::frame::{RunFrame, RunPhase, WaitingOn};
use yunta_testkit::Bench;
use yunta_testkit_core::FixedClock;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, News, Operation, Outcome, Performer, Request, State};

/// A workspace working in `dir`, its state kept in `root` and its org layer there too,
/// and a `PATH` git stands on.
pub fn workspace(dir: &Path, root: &Path) -> Workspace {
    let env = Env {
        yunta_home: Some(root.to_path_buf()),
        org_config: Some(root.join("org.yaml")),
        path: vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
        ..Env::default()
    };
    Workspace {
        dir: dir.to_path_buf(),
        env,
        clock: Arc::new(FixedClock),
        ids: Arc::new(SystemIdSource),
        secrets: Arc::new(ProcessSecrets),
        binary: PathBuf::from("yunta"),
    }
}

/// The instant the application stands at in a test.
pub fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000, 0).expect("an instant")
}

/// `state` driven with `performer`: what `asked` asks for and every request it leads to
/// carried out, each answer handed back, then each of `strokes` in turn once the state
/// waits on nothing; the state as it stands when it waits on nothing and no stroke is
/// left, with what only a surface does, showing a page or leaving, that it asked for.
pub async fn drive(
    mut state: State,
    asked: Vec<Request>,
    performer: &Arc<Performer>,
    strokes: &[Stroke],
) -> (State, Vec<Request>) {
    let (inputs, mut received) = mpsc::unbounded_channel();
    let mut strokes = strokes.iter().copied();
    let (mut asked, mut surfaced) = (asked, Vec::new());
    loop {
        for request in asked.drain(..) {
            surfaced.extend(performer.take(request, &inputs));
        }
        if state.waiting_on().is_empty() {
            match strokes.next() {
                Some(stroke) => asked = state.update(Input::Stroke(stroke)),
                None => return (state, surfaced),
            }
            continue;
        }
        let input = received
            .recv()
            .await
            .expect("the performer hands back what the state waits on");
        asked = state.update(input);
    }
}

/// A run a person stopped and nobody closed, which a field lists under what is stopped.
pub fn stopped(run: &RunId) -> FieldRow {
    FieldRow {
        frame: RunFrame {
            standing: Standing::Stopped,
            phase: RunPhase::Waiting {
                on: WaitingOn::Run {
                    reason: PauseReason::Cancelled,
                },
            },
            ..RunFrame::sample(run)
        },
        menu: false,
    }
}

/// The application open on a field of `rows`.
pub fn on_the_field(rows: Vec<FieldRow>) -> State {
    let (mut state, asked) = State::opened(now());
    let [Request::Perform(opening, _)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    let asked = state.update(Input::Outcome(
        *opening,
        Outcome::Repository(Repository::NeverSetUp),
    ));
    let [Request::Perform(
        reading,
        Operation::Field {
            reach: FieldReach::Here,
        },
    )] = asked.as_slice()
    else {
        panic!("in a repository the field is read: {asked:?}");
    };
    let reading_outcome = Outcome::Field {
        reading: FieldReading {
            rows,
            unreadable: Vec::new(),
            elsewhere: 0,
            here: true,
        },
        storage: PathBuf::from("/home/me/.yunta"),
    };
    state.update(Input::Outcome(*reading, reading_outcome));
    state
}

/// What `operation` hands back, performed in `bench`'s world: what it finds while it
/// lasts, then what it comes to.
pub async fn performed(bench: &Bench, operation: Operation) -> (Vec<News>, Outcome) {
    let root = bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root");
    let performer = Performer::new(workspace(&bench.worktree, root));
    // A ticket of the application's own: the performer hands back under what it is given.
    let (_, requests) = State::opened(now());
    let Some(Request::Perform(ticket, _)) = requests.into_iter().next() else {
        panic!("the application asks something as it opens");
    };
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
