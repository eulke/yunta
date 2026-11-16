//! What a run's sessions say, heard off the socket its engine serves while the run is
//! followed: each word handed back as news of the run, with how many the engine dropped
//! for a reader that fell behind.

use std::sync::Arc;

use tempfile::TempDir;
use yunta_api::{FollowedRun, Following, RunNews};
use yunta_core::port::Said;
use yunta_core::{NodeId, RunId, Seq, SessionId};
use yunta_engine::said::{Socket, Voices};
use yunta_engine::ProcessRegistry;

use super::{driven_by_this_process, next_of, whole, World};

/// How many words a test says at once while the follower reads none: more than the
/// engine keeps for a reader that falls behind, which is a few hundred.
const BURST: u64 = 1_000;

/// What one of `run`'s sessions says, `text`.
fn said(run: &RunId, text: &str) -> Said {
    Said {
        run_id: run.clone(),
        node_id: NodeId::from("only"),
        session_id: SessionId::from_static("session-1"),
        task_id: None,
        seq: Seq::from(1),
        at: chrono::DateTime::from_timestamp(1_800_000_000, 0).unwrap_or_default(),
        text: text.to_string(),
        cut: false,
    }
}

/// The words `engine` serves, on a socket its registry names, in a directory short
/// enough for a socket's address and kept for as long as the directory is held.
fn served(engine: &ProcessRegistry) -> (Arc<Voices>, TempDir) {
    let socket_dir = tempfile::Builder::new()
        .prefix("w")
        .tempdir_in("/tmp")
        .unwrap();
    let voices = Arc::new(Voices::new());
    let path = socket_dir.path().join("words.sock");
    let socket = Socket::bind(&path, Arc::clone(&voices), yunta_core::process::user::uid());
    engine.serve_words(socket.unwrap());
    (voices, socket_dir)
}

#[tokio::test]
async fn what_a_session_says_on_the_runs_socket_is_handed_back() {
    let world = World::new();
    let (run, dir) = world.born("speaking").await;
    let engine = driven_by_this_process(&dir);
    let (voices, _socket_dir) = served(&engine);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    voices.hand(said(&run, "reading the plan"));
    let RunNews::Said { said: heard, .. } = next_of(&mut following, &mut kept).await else {
        panic!("what was said comes back");
    };
    assert_eq!(heard.text, "reading the plan");
}

/// The next word `following` hands, with how many words it missed just before it.
async fn word(following: &mut Following, kept: &mut FollowedRun) -> (Said, u64) {
    match next_of(following, kept).await {
        RunNews::Said { said, missed } => (said, missed),
        other => panic!("only words come, and came {other:?}"),
    }
}

/// Words said all at once, more than the engine keeps for a reader, while the reader
/// reads none: on one thread, nothing reads until the test awaits. Each word is handed
/// back, or counted in the `missed` of the next one handed.
#[tokio::test(flavor = "current_thread")]
async fn a_slow_reader_is_told_how_many_words_it_missed() {
    let world = World::new();
    let (run, dir) = world.born("slow").await;
    let engine = driven_by_this_process(&dir);
    let (voices, _socket_dir) = served(&engine);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    voices.hand(said(&run, "listening"));
    let (first, missed) = word(&mut following, &mut kept).await;
    assert_eq!((first.text.as_str(), missed), ("listening", 0));

    for at in 0..BURST {
        voices.hand(said(&run, &format!("word {at}")));
    }
    let last = format!("word {}", BURST - 1);
    let (mut handed, mut missed) = (0, 0);
    loop {
        let (heard, lost) = word(&mut following, &mut kept).await;
        handed += 1;
        missed += lost;
        if heard.text == last {
            break;
        }
    }
    assert!(
        missed > 0,
        "the reader that fell behind is told it missed words"
    );
    assert_eq!(
        handed + missed,
        BURST,
        "each word is handed or counted missed"
    );
}
