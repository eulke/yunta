//! What the interface's own tests share beyond the world they work in
//! (`yunta_testkit_ui::sandbox`): a run a field lists as stopped, the application open on a
//! field of runs a test makes up, and a piece of work performed in a bench's world.
//!
//! Every test binary that declares `mod support` compiles the whole module and uses a
//! part of it, so what another binary needs reads as dead here.
#![allow(dead_code)]

use std::path::PathBuf;

use surco::words::Standing;
use tokio::sync::mpsc;
use yunta_api::{FieldReach, FieldReading, Repository};
use yunta_core::events::PauseReason;
use yunta_core::RunId;
use yunta_render::page::FieldRow;
use yunta_replay::frame::{RunFrame, RunPhase, WaitingOn};
use yunta_testkit::Bench;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::{Input, News, Operation, Outcome, Performer, Request, State};

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
        Outcome::Repository(Repository::SetUp),
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
