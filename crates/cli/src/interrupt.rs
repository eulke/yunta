//! What trips this invocation's cancellation, in two stages (D181).
//!
//! The first interrupt stops the work: every subprocess the invocation
//! spawned to do something answers to it. The second aborts what
//! stopping still holds — the git that gives a checkout's lock back, the
//! hand-over to a detached process — because those run *because* the
//! first one fired, and one token for both would kill the cleanup with
//! the work.
//!
//! The source is injected: the process's SIGINT for a real command,
//! nothing for a test's, the same two tokens for a `Context` derived
//! from another. A value the composition root is handed, never a process
//! global.

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// The two stages of this invocation's interruption, and the task that
/// trips them.
pub(crate) struct Interrupt {
    stop: CancellationToken,
    abort: CancellationToken,
    /// The listener, kept so it dies with the `Interrupt` that owns it.
    /// A derived `Interrupt` has none: only the root installs a stream.
    listener: Option<JoinHandle<()>>,
}

impl Interrupt {
    /// The process's own SIGINT, wired to the two stages.
    ///
    /// The stream is installed here, synchronously, so the handler
    /// exists the moment this returns — a signal that arrives while the
    /// project is still resolving is one this invocation catches, not
    /// one that kills it halfway through.
    pub(crate) fn ctrl_c() -> std::io::Result<Self> {
        let signals = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        Ok(Self::driven_by(signals))
    }

    /// No source at all: the tokens exist and nothing ever trips them.
    /// What a test builds when the interruption is not what it is
    /// asserting about — every real invocation has a source (D181).
    #[cfg(test)]
    pub(crate) fn never() -> Self {
        Interrupt {
            stop: CancellationToken::new(),
            abort: CancellationToken::new(),
            listener: None,
        }
    }

    /// The same two stages for a `Context` derived from this one — a
    /// `yunta test` sandbox, a request the control plane handles — with
    /// no listener of its own, so one stream serves the whole process.
    pub(crate) fn shared(&self) -> Self {
        Interrupt {
            stop: self.stop.clone(),
            abort: self.abort.clone(),
            listener: None,
        }
    }

    /// The token everything this invocation spawned to work answers to.
    pub(crate) fn stop(&self) -> &CancellationToken {
        &self.stop
    }

    /// The token what gives a take back answers to: it survives the
    /// first interrupt and falls to the second.
    pub(crate) fn abort(&self) -> &CancellationToken {
        &self.abort
    }

    /// Trips the stages from `source`, one interrupt at a time.
    fn driven_by(mut source: impl Interrupts) -> Self {
        let stop = CancellationToken::new();
        let abort = CancellationToken::new();
        let (stopping, aborting) = (stop.clone(), abort.clone());
        let listener = tokio::spawn(async move {
            if source.next().await.is_none() {
                return;
            }
            stopping.cancel();
            if source.next().await.is_none() {
                return;
            }
            aborting.cancel();
            // Nothing after the second: this invocation has said
            // everything it has to say about being interrupted, and
            // what is left is the command unwinding.
        });
        Interrupt {
            stop,
            abort,
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
    async fn a_fired_interrupt_cancels_the_contexts_token() {
        let (source, receiver) = mpsc::channel(2);
        let interrupt = Interrupt::driven_by(receiver);
        assert!(!interrupt.stop().is_cancelled());
        assert!(!interrupt.abort().is_cancelled());

        source.send(()).await.expect("the listener is waiting");
        interrupt.stop().cancelled().await;
        assert!(
            !interrupt.abort().is_cancelled(),
            "the first interrupt stops the work and leaves the cleanup alone"
        );

        source.send(()).await.expect("the listener is waiting");
        interrupt.abort().cancelled().await;
    }

    #[tokio::test]
    async fn an_interrupt_nothing_drives_never_trips() {
        let interrupt = Interrupt::never();
        assert!(!interrupt.stop().is_cancelled());
        assert!(!interrupt.abort().is_cancelled());
    }

    /// A derived `Interrupt` is the same interruption seen from a
    /// `Context` of its own: one stream, one pair of stages.
    #[tokio::test]
    async fn a_shared_interrupt_trips_with_the_one_that_shared_it() {
        let (source, receiver) = mpsc::channel(2);
        let root = Interrupt::driven_by(receiver);
        let derived = root.shared();

        source.send(()).await.expect("the listener is waiting");
        derived.stop().cancelled().await;
    }
}
