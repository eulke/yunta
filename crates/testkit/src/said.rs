//! What a run's sessions said, as a test reads it off the run's socket: from
//! the engine in the test's own process, or from a `yunta` it started.

use std::path::{Path, PathBuf};

use yunta_engine::said::{connect, Heard};
use yunta_engine::{read_registry, Registry};

use crate::wait::{wait_for_async, WAIT_DEADLINE};

/// The socket the registry of the run under `run_dir` names, once it names
/// one.
pub async fn socket_of(run_dir: &Path) -> PathBuf {
    wait_for_async(
        || {
            let named = named(run_dir);
            async move { named }
        },
        || {
            format!(
                "the registry under {} never named a socket",
                run_dir.display()
            )
        },
    )
    .await
}

/// The socket the registry under `run_dir` names now, if it names one.
fn named(run_dir: &Path) -> Option<PathBuf> {
    match read_registry(run_dir) {
        Registry::Read(registry) => registry.doc.words.clone(),
        Registry::Absent | Registry::Corrupt(_) => None,
    }
}

/// The first `count` frames the socket of the run under `run_dir` serves,
/// connecting once its registry names it. Fewer when the socket closes
/// first.
pub async fn heard(run_dir: &Path, count: usize) -> Vec<Heard> {
    let socket = socket_of(run_dir).await;
    let mut hearing = connect(&socket)
        .await
        .unwrap_or_else(|error| panic!("{} accepts no reader: {error}", socket.display()));
    let mut heard = Vec::new();
    while heard.len() < count {
        let next = tokio::time::timeout(WAIT_DEADLINE, hearing.next())
            .await
            .unwrap_or_else(|_| panic!("the socket served {} of {count} frames", heard.len()));
        let Some(next) = next else {
            break;
        };
        heard.push(next);
    }
    heard
}

/// [`heard`], for a test with no runtime of its own, such as one that drives
/// the `yunta` binary.
pub fn heard_blocking(run_dir: &Path, count: usize) -> Vec<Heard> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime to read the socket on")
        .block_on(heard(run_dir, count))
}
