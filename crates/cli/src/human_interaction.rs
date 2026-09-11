//! `ConsoleInteraction` — the terminal implementation of
//! `yunta_engine::HumanInteraction`, and the border where a prompt that
//! produced no answer becomes a line a person reads.
//!
//! It never auto-decides. Without a console to prompt on (piped,
//! redirected, no terminal at either end) it reports "can't interact"
//! rather than guessing, and the engine parks the run — the same answer
//! a person gives by pressing Escape at the prompt itself.
//!
//! Every prompt runs on a blocking thread. A person taking their time
//! would otherwise freeze the run's single-threaded runtime and every
//! task sharing it: the per-node run-tools listener, the Ctrl-C handler
//! that stops the run, the observer that draws it.
//!
//! Two things a prompt shares the terminal with are settled here, in
//! the one place every prompt passes through. The run's live surface
//! stands down for as long as a prompt is open, so no row of it lands
//! on what a person is reading. And the read is raced against the run's
//! own cancellation, so a run stopped from outside stops waiting on a
//! person who is no longer being asked anything.

use async_trait::async_trait;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{GateWaitingPayload, HumanChoice};
use yunta_core::QuestionsFile;
use yunta_engine::{HumanInteraction, QuestionsReply};

use crate::ask::{answer, decide, Answered, Console, NoAnswer};
use crate::error::{note, warn};
use crate::surface::Curtain;

/// The console surface of one invocation, holding what a prompt has to
/// take its turn with.
pub struct ConsoleInteraction {
    /// What tells the run's live surface to stand down while a prompt
    /// has the terminal, and to come back when it ends.
    curtain: Curtain,
    /// The run's own cancellation. Stopping a run is not answering it:
    /// a cancelled run stops waiting here, and the engine unwinds it the
    /// way every other cancellation path unwinds it.
    cancel: CancellationToken,
}

impl ConsoleInteraction {
    /// The surface for a run drawn under `curtain` and stopped by
    /// `cancel`.
    pub fn new(curtain: Curtain, cancel: CancellationToken) -> Self {
        Self { curtain, cancel }
    }

    /// Runs one prompt to its end, off the runtime, and reports what it
    /// produced.
    ///
    /// `None` is every way a prompt ends without an answer, said once
    /// here so each reason reaches the person in its own words and the
    /// engine in the one shape it acts on: park the run, record nothing.
    async fn prompted<T, P>(&self, prompt: P) -> Option<T>
    where
        T: Send + 'static,
        P: FnOnce(&Console) -> Answered<T> + Send + 'static,
    {
        let console = Console::open()?;
        self.curtain.lower().await;
        let _turn = Turn(&self.curtain);
        let reading = tokio::task::spawn_blocking(move || prompt(&console));
        self.read(reading).await.and_then(settled)
    }

    /// What `reading` answered, or `None` once the run is cancelled out
    /// from under it.
    ///
    /// The abandoned read is the one task this process does not keep a
    /// handle on, and it is deliberate: a thread parked in a terminal
    /// read cannot be told to stop, and the key that would free it is
    /// one nobody is going to press now that the run is stopping. So
    /// the engine gets its answer, unwinds the run and kills the tree,
    /// and `main` leaves without waiting on the read.
    ///
    /// What that read took from the terminal is not put back here. The
    /// thread is still running, and it turns raw mode back on between
    /// its own reads, so the last word on what the terminal is left in
    /// belongs to the process: `main` hands it back through
    /// [`crate::ask::restore_terminal`] once nothing is left to take it
    /// again.
    async fn read<T>(&self, reading: JoinHandle<Answered<T>>) -> Option<Answered<T>> {
        tokio::select! {
            read = reading => Some(read.unwrap_or_else(|failed| Err(NoAnswer::Failed(failed.to_string())))),
            () = self.cancel.cancelled() => None,
        }
    }
}

#[async_trait]
impl HumanInteraction for ConsoleInteraction {
    async fn resolve(&self, escalation: &GateWaitingPayload) -> Option<HumanChoice> {
        let escalation = escalation.clone();
        self.prompted(move |console| decide(console, &escalation))
            .await
    }

    /// Question by question, each answered the way its own
    /// `answer_type` is answered. `interactive` goes unread: it asks a
    /// surface that can hold a conversation to hold one, and this
    /// surface reads a `kind: questions` artifact and nothing else.
    async fn ask(&self, questions: &QuestionsFile, _interactive: bool) -> Option<QuestionsReply> {
        let questions = questions.clone();
        self.prompted(move |console| answer(console, &questions))
            .await
    }
}

/// The terminal, held for as long as one prompt is open.
///
/// The surface gets it back when the prompt ends, whichever way it ends
/// — answered, declined, interrupted, unreadable, or a caller that
/// stopped awaiting the prompt altogether. A region that stayed down
/// would leave the rest of the run undrawn.
struct Turn<'a>(&'a Curtain);

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.0.raise();
    }
}

/// What a finished prompt produced, with every ending that produced no
/// answer put to the person in its own words.
fn settled<T>(read: Answered<T>) -> Option<T> {
    match read {
        Ok(answered) => Some(answered),
        Err(NoAnswer::Declined) => {
            note("nothing recorded — the run parks here, with its state intact, until it resumes");
            None
        }
        // The interrupt is already on its way to the run's own
        // cancellation; a second account of it here would be a second
        // story about whether a person stopped the run.
        Err(NoAnswer::Interrupted) => None,
        Err(NoAnswer::OffMenu) => {
            warn("the prompt answered with an option that was not on the menu — nothing recorded");
            None
        }
        Err(NoAnswer::Unreadable(e)) => {
            warn(format!("could not read your answer from the terminal: {e}"));
            None
        }
        Err(NoAnswer::Failed(e)) => {
            warn(format!("the thread holding the prompt failed: {e}"));
            None
        }
    }
}
