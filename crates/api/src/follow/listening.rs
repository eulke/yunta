//! See [`super`]. What the run's sessions say, heard off the run's socket while it is
//! followed.
//!
//! The run's registry names its socket while an engine serves one, and a following
//! reads the registry once a second. Each look at the run says what to listen on: the
//! socket the registry last named, while nobody hears it, or nothing when it names none.
//! A refused connection, such as a socket a dead engine left, is no words, and the next
//! look tries again: a connection refused costs nothing worth spacing out. What is heard
//! is handed to the following with how many words the engine dropped before it, which
//! the following hands back as news beside what the log says.

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use yunta_engine::said::Heard;

use super::Words;

/// A following's one reader of a run's socket, if it has one.
pub(super) struct Listening {
    /// Where what is heard goes.
    ear: mpsc::UnboundedSender<Heard>,
    /// The socket listened on, and the task that reads it.
    on: Option<(PathBuf, JoinHandle<()>)>,
}

impl Listening {
    /// Listening on nothing yet, handing what it hears to `ear`.
    pub(super) fn handing_to(ear: mpsc::UnboundedSender<Heard>) -> Self {
        Listening { ear, on: None }
    }

    /// Listens on `socket`: from now on when it is a socket nobody hears here, and again
    /// when its reader has ended. With no socket, it stops listening.
    pub(super) fn on(&mut self, socket: Option<&Path>) {
        let hearing = self
            .on
            .as_ref()
            .is_some_and(|(heard, task)| Some(heard.as_path()) == socket && !task.is_finished());
        if hearing {
            return;
        }
        self.stop();
        let Some(socket) = socket else {
            return;
        };
        let (path, ear) = (socket.to_path_buf(), self.ear.clone());
        let task = tokio::spawn(async move {
            let Some(mut words) = Words::hear(&path).await else {
                return;
            };
            while let Some(heard) = words.next().await {
                if ear.send(heard).is_err() {
                    return;
                }
            }
        });
        self.on = Some((socket.to_path_buf(), task));
    }

    fn stop(&mut self) {
        if let Some((_, task)) = self.on.take() {
            task.abort();
        }
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.stop();
    }
}
