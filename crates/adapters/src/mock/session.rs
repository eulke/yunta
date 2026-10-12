//! The handle a played session is returned as: the stream the engine reads,
//! the two stops that end its player, and the hold on what its launch set up.
//! Dropping it ends the player; releasing it takes back the launch's mounts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::{self, BoxStream};
use tokio::sync::{mpsc, Notify};
use yunta_core::events::SessionExit;
use yunta_core::port::{AgentEvent, AgentSession};
use yunta_core::process::subprocess::Held;

use super::script::{play, Script, Stops};

/// The session a claimed script is played by: the stream the engine reads
/// and the handles that stop it. Dropping it ends its player, and what its
/// launch set up is taken back.
pub(super) struct MockSession {
    receiver: Option<mpsc::UnboundedReceiver<AgentEvent>>,
    interrupt: Arc<Notify>,
    kill: Arc<Notify>,
    /// The task playing this session's script. A session that ends
    /// before its script does — a cancelled run, a caller that drops
    /// the stream — leaves a player waiting on a step that will never
    /// be read, so the session owns it and takes it down with itself.
    player: tokio::task::JoinHandle<()>,
    /// Shared with the adapter that opened this session.
    interrogated: Arc<AtomicBool>,
    /// What the launch set up for this session, taken back on release.
    held: Option<Box<dyn Held>>,
}

/// Starts playing `script` as a session, which holds what its launch set up
/// until it is released.
pub(super) fn start(
    script: Script,
    held: Box<dyn Held>,
    interrogated: Arc<AtomicBool>,
) -> Box<dyn AgentSession> {
    let (events, receiver) = mpsc::unbounded_channel();
    let interrupt = Arc::new(Notify::new());
    let kill = Arc::new(Notify::new());
    let stops = Stops::of(&script.outcome, Arc::clone(&interrupt), Arc::clone(&kill));
    let player = tokio::spawn(play(script, events, stops));
    Box::new(MockSession {
        receiver: Some(receiver),
        interrupt,
        kill,
        player,
        interrogated,
        held: Some(held),
    })
}

impl Drop for MockSession {
    fn drop(&mut self) {
        self.player.abort();
    }
}

#[async_trait]
impl AgentSession for MockSession {
    fn events(&mut self) -> BoxStream<'_, AgentEvent> {
        match self.receiver.take() {
            Some(rx) => Box::pin(stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|event| (event, rx))
            })),
            // A second call gets an already-exhausted stream rather than a
            // panic — the caller's misuse, not a reason to crash the run.
            None => Box::pin(stream::empty()),
        }
    }

    async fn interrupt(&mut self) -> yunta_core::Result<()> {
        self.interrupt.notify_one();
        Ok(())
    }

    async fn kill(&mut self) -> yunta_core::Result<()> {
        self.kill.notify_one();
        Ok(())
    }

    /// Takes back what the launch set up for this session.
    async fn release(&mut self) -> yunta_core::Result<()> {
        match self.held.take() {
            Some(held) => held.release(),
            None => Ok(()),
        }
    }

    /// Nothing, because there is no process — and a record that the
    /// question was put, which is what a test of the engine's rule
    /// reads.
    async fn exit(&mut self) -> yunta_core::Result<Option<SessionExit>> {
        self.interrogated.store(true, Ordering::SeqCst);
        Ok(None)
    }
}
