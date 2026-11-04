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

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{GateWaitingPayload, HumanChoice};
use yunta_core::{Env, QuestionsFile};
use yunta_engine::{HumanInteraction, QuestionsReply};

use crate::ask::{answer, decide, Answered, Ask, Console, Escape, NoAnswer, Opens};
use crate::error::{note, warn};
use crate::surface::{Curtain, Diagnostics};

/// The console surface of one invocation, holding what a prompt has to
/// take its turn with.
pub struct ConsoleInteraction {
    /// What tells the run's live surface to stand down while a prompt
    /// has the terminal, and to come back when it ends.
    curtain: Curtain,
    /// The door a line goes out through while the region is still up —
    /// what opening the console has to say when it cannot open one.
    diagnostics: Diagnostics,
    /// The run's own cancellation. Stopping a run is not answering it:
    /// a cancelled run stops waiting here, and the engine unwinds it the
    /// way every other cancellation path unwinds it.
    cancel: CancellationToken,
    /// The ambient environment the answers are attributed by: a copy of
    /// the invocation's, so a prompt never reads the process again.
    env: Env,
    /// Whether the run's own surface says what was answered, and by whom. Under `--quiet` it
    /// does not, so the prompt that took the answer says it.
    chronicled: bool,
}

impl ConsoleInteraction {
    /// The surface for a run drawn under `curtain`, saying what it has
    /// to say through `diagnostics`, stopped by `cancel`, and attributing
    /// what a person answers by `env`, and whether the run's surface says what it was.
    pub fn new(
        curtain: Curtain,
        diagnostics: Diagnostics,
        cancel: CancellationToken,
        env: Env,
        chronicled: bool,
    ) -> Self {
        Self {
            curtain,
            diagnostics,
            cancel,
            env,
            chronicled,
        }
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
        let console = Console::open(&self.diagnostics, Escape::Parks)
            .await?
            .chronicled_by_the_run(self.chronicled);
        let stop = console.stopper();
        self.curtain.lower().await;
        let _turn = Turn(&self.curtain);
        let reading = tokio::task::spawn_blocking(move || prompt(&console));
        self.read(reading, stop).await.and_then(settled)
    }

    /// What `reading` answered, or `None` once the run is cancelled out from under it.
    ///
    /// A cancelled run tells the prompt to stop and waits for it: within a tick of
    /// its read the prompt has taken what it drew off the screen and handed the
    /// terminal back, so what the run says next is not drawn under a question nobody
    /// can answer any more.
    async fn read<T>(
        &self,
        mut reading: JoinHandle<Answered<T>>,
        stop: Arc<AtomicBool>,
    ) -> Option<Answered<T>> {
        tokio::select! {
            read = &mut reading => {
                Some(read.unwrap_or_else(|failed| Err(NoAnswer::Failed(failed.to_string()))))
            }
            () = self.cancel.cancelled() => {
                stop.store(true, Ordering::Release);
                drop(reading.await);
                None
            }
        }
    }
}

#[async_trait]
impl HumanInteraction for ConsoleInteraction {
    fn present(&self) -> bool {
        Console::attended()
    }

    async fn resolve(&self, escalation: &GateWaitingPayload) -> Option<HumanChoice> {
        let escalation = escalation.clone();
        let env = self.env.clone();
        self.prompted(move |console| {
            decide(
                console,
                &env,
                Ask {
                    escalation: &escalation,
                    shown: None,
                    changed: None,
                    routes: BTreeMap::new(),
                    node: None,
                    called: None,
                    opens: Opens::TheCard,
                },
            )
        })
        .await
    }

    /// The decision, held as a card: the documents it is about drawn above it, what
    /// the run changed and what follows each answer worked out here, where the
    /// workflow and the run can be read, before the blocking read starts.
    async fn resolve_in(
        &self,
        escalation: &GateWaitingPayload,
        asking: &yunta_engine::Asking<'_>,
    ) -> Option<HumanChoice> {
        let routes =
            yunta_render::gate::routes(escalation, asking.workflow, asking.left_out, asking.node);
        let called = Some(crate::render::called(&asking.workflow.name, asking.mode));
        let escalation = escalation.clone();
        let shown = asking.shown.to_vec();
        let run = asking.run.clone();
        let changed = asking.changed.cloned();
        let node = asking.node.cloned();
        let env = self.env.clone();
        self.prompted(move |console| {
            decide(
                console,
                &env,
                Ask {
                    escalation: &escalation,
                    shown: Some((&run, &shown)),
                    changed: changed.as_ref(),
                    routes,
                    node: node.as_ref(),
                    called,
                    opens: Opens::TheCard,
                },
            )
        })
        .await
    }

    /// Question by question, each answered the way its own
    /// `answer_type` is answered.
    async fn ask(
        &self,
        node: &yunta_core::NodeId,
        questions: &QuestionsFile,
    ) -> Option<QuestionsReply> {
        let questions = questions.clone();
        let node = node.clone();
        let env = self.env.clone();
        self.prompted(move |console| answer(console, &env, &node, &questions))
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
            note("nothing recorded: the run parks here, with its state intact, until it resumes");
            None
        }
        // The interrupt is already on its way to the run's own
        // cancellation; a second account of it here would be a second
        // story about whether a person stopped the run.
        Err(NoAnswer::Interrupted) => None,
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
