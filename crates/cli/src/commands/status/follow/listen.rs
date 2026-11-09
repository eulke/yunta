//! What the run's sessions say, heard off the run's socket while it is
//! followed.
//!
//! The run's registry names its socket while an engine serves one, and the
//! follower reads the registry once a second. Each reading says what to
//! listen to: a socket the registry names and this follower does not hear
//! yet, or nothing when it names none. A refused connection, such as a
//! socket a dead engine left, is no words, and the next reading tries again.
//! What is heard reaches the surface through its observer, the same door the
//! engine of `yunta run`'s own process hands it through, so the region does
//! not know which feed a frame came by.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::task::JoinHandle;
use yunta_core::port::RunObserver;

/// The follower's one reader of a run's socket, if it has one.
#[derive(Default)]
pub(super) struct Listening {
    /// The socket listened to, and the task that reads it.
    to: Option<(PathBuf, JoinHandle<()>)>,
}

impl Listening {
    /// Listens to `socket`, handing what is heard to `ear`: from now on when
    /// it is a socket this follower does not hear, and again when its reader
    /// has ended. With no socket or no ear, it stops listening.
    pub(super) fn to(&mut self, socket: Option<&Path>, ear: Option<Arc<dyn RunObserver>>) {
        let hearing = self
            .to
            .as_ref()
            .is_some_and(|(heard, task)| Some(heard.as_path()) == socket && !task.is_finished());
        if hearing {
            return;
        }
        self.stop();
        let (Some(socket), Some(ear)) = (socket, ear) else {
            return;
        };
        let path = socket.to_path_buf();
        let task = tokio::spawn(async move {
            let Some(mut words) = yunta_api::Words::hear(&path).await else {
                return;
            };
            while let Some(said) = words.next().await {
                ear.said(&said);
            }
        });
        self.to = Some((socket.to_path_buf(), task));
    }

    fn stop(&mut self) {
        if let Some((_, task)) = self.to.take() {
            task.abort();
        }
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.stop();
    }
}
