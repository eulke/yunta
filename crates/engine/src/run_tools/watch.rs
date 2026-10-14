//! The watcher of a session's listener: it records the listener's death
//! when the server ended of a failure, under the session that listener serves.

use tokio::task::{AbortHandle, JoinHandle};
use yunta_core::rail::hook::RailFailureCause;

use super::session::SessionTools;

/// Hands `server` to a watcher task, which records the listener's death when
/// it died of a failure: an I/O error, or a panic. A server the session
/// aborted, or one a graceful shutdown ended, died of nothing. Returns the
/// handle that stops the server.
pub(super) fn watch_server(
    server: JoinHandle<std::io::Result<()>>,
    tools: SessionTools,
) -> AbortHandle {
    let abort = server.abort_handle();
    tokio::spawn(async move {
        if died_of(server.await) {
            let session = tools.rails.current_owner();
            tools
                .failed(session.as_ref(), None, RailFailureCause::ListenerDied)
                .await;
        }
    });
    abort
}

/// Whether a server ended of a failure, given how its task ended: an I/O
/// error it returned, or a panic. A graceful end and an abort are no death.
fn died_of(ended: Result<std::io::Result<()>, tokio::task::JoinError>) -> bool {
    match ended {
        Ok(Ok(())) => false,
        Ok(Err(_)) => true,
        Err(join) => !join.is_cancelled(),
    }
}

#[cfg(test)]
mod death_tests {
    use super::died_of;

    fn panicking() -> std::io::Result<()> {
        panic!("the server panicked")
    }

    #[tokio::test]
    async fn a_server_that_fails_or_panics_died_and_one_that_ends_or_is_aborted_did_not() {
        let failed = tokio::spawn(async { Err(std::io::Error::other("the accept loop broke")) });
        assert!(died_of(failed.await));
        let panicked = tokio::spawn(async { panicking() });
        assert!(died_of(panicked.await));
        let ended = tokio::spawn(async { Ok(()) });
        assert!(!died_of(ended.await));
        let aborted = tokio::spawn(std::future::pending::<std::io::Result<()>>());
        aborted.abort();
        assert!(!died_of(aborted.await));
    }
}
