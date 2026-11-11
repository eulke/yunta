//! The performer holds work as under way the moment it is asked, before its task first
//! runs, so a let-go that follows at once finds the work and stops it.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use yunta_api::Workspace;
use yunta_core::{Env, ProcessSecrets, SystemIdSource};
use yunta_testkit_core::FixedClock;

use super::Performer;
use crate::{Operation, Request, State};

/// A performer working in `dir`, which holds no repository.
fn performer(dir: PathBuf) -> Arc<Performer> {
    Arc::new(Performer::new(Workspace {
        dir,
        env: Env::default(),
        clock: Arc::new(FixedClock),
        ids: Arc::new(SystemIdSource),
        secrets: Arc::new(ProcessSecrets),
        binary: PathBuf::from("yunta"),
    }))
}

#[tokio::test(flavor = "current_thread")]
async fn work_let_go_the_moment_after_it_is_asked_is_stopped_where_it_stands() {
    let dir = tempfile::tempdir().expect("a directory");
    let performer = performer(dir.path().to_path_buf());
    let (_, requests) =
        State::opened(chrono::DateTime::from_timestamp(1_800_000_000, 0).expect("an instant"));
    let Some(Request::Perform(ticket, _)) = requests.into_iter().next() else {
        panic!("the application asks something as it opens");
    };
    let (inputs, _received) = mpsc::unbounded_channel();
    performer.take(Request::Perform(ticket, Operation::ReadRepository), &inputs);
    // The task has not run yet on this thread: the work is held as under way already.
    assert!(performer.held().under_way(ticket));
    performer.take(Request::LetGo(ticket), &inputs);
    assert!(!performer.held().under_way(ticket), "let go, it is stopped");
}
