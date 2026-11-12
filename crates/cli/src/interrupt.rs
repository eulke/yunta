//! What trips this invocation's cancellation, in two stages.
//!
//! The first interrupt stops the work: every subprocess the invocation
//! spawned to do something answers to it. The second aborts what
//! stopping still holds — the git that gives a checkout's lock back, the
//! hand-over to a detached process — because those run *because* the
//! first one fired, and one token for both would kill the cleanup with
//! the work.
//!
//! The source is injected: the process's SIGINT, which the shell installs
//! for a real command, and nothing for a test's. A value the composition
//! root is handed, never a process global.

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use yunta_api::Stop;

/// The two stages of this invocation's interruption, and the task that
/// trips them.
pub(crate) struct Interrupt {
    /// The two stages, as the API reads them.
    stages: Stop,
    /// The listener, kept so it dies with the `Interrupt` that owns it;
    /// none when nothing drives the stages.
    listener: Option<JoinHandle<()>>,
}

impl Interrupt {
    /// No source at all: the tokens exist and nothing ever trips them.
    /// What a test builds when the interruption is not what it is
    /// asserting about — every real invocation has a source.
    #[cfg(test)]
    pub(crate) fn never() -> Self {
        Interrupt {
            stages: Stop::new(),
            listener: None,
        }
    }

    /// The two stages, as an operation of the API is handed them.
    pub(crate) fn stages(&self) -> Stop {
        self.stages.clone()
    }

    /// Trips the stages from `source`, one interrupt at a time.
    pub(crate) fn driven_by(mut source: impl Interrupts) -> Self {
        let stages = Stop::new();
        let tripped = stages.clone();
        let listener = tokio::spawn(async move {
            if source.next().await.is_none() {
                return;
            }
            tripped.stop();
            if source.next().await.is_none() {
                return;
            }
            tripped.abort();
            // Nothing after the second: this invocation has said
            // everything it has to say about being interrupted, and
            // what is left is the command unwinding.
        });
        Interrupt {
            stages,
            listener: Some(listener),
        }
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        if let Some(listener) = self.listener.take() {
            listener.abort();
        }
    }
}

/// Where the interrupts come from. The process's SIGINT stream in a real
/// invocation; a channel a unit test sends on.
pub(crate) trait Interrupts: Send + 'static {
    /// The next interrupt, or `None` when no more can arrive.
    fn next(&mut self) -> impl std::future::Future<Output = Option<()>> + Send;
}

impl Interrupts for tokio::signal::unix::Signal {
    async fn next(&mut self) -> Option<()> {
        self.recv().await
    }
}

impl Interrupts for mpsc::Receiver<()> {
    async fn next(&mut self) -> Option<()> {
        self.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_fired_interrupt_stops_the_work_then_the_teardown() {
        let (source, receiver) = mpsc::channel(2);
        let interrupt = Interrupt::driven_by(receiver);
        assert!(!interrupt.stages().work().is_cancelled());
        assert!(!interrupt.stages().teardown().is_cancelled());

        source.send(()).await.expect("the listener is waiting");
        interrupt.stages().work().cancelled().await;
        assert!(
            !interrupt.stages().teardown().is_cancelled(),
            "the first interrupt stops the work and leaves the cleanup alone"
        );

        source.send(()).await.expect("the listener is waiting");
        interrupt.stages().teardown().cancelled().await;
    }

    #[tokio::test]
    async fn an_interrupt_nothing_drives_never_trips() {
        let interrupt = Interrupt::never();
        assert!(!interrupt.stages().work().is_cancelled());
        assert!(!interrupt.stages().teardown().is_cancelled());
    }
}
