//! See [`super`]. What the run read whole waits on a person for, read once for each pause.
//!
//! What a run asks is keyed by the [`AnswerablePause`] it waits at: a decision, by the
//! node or run `awaits_decision` names and the seq of the run's pause; a round of
//! questions, by its node and the seq of its `questions_asked`, which stays the same
//! while the run parks again on the same round. Each key says that event's seq, so an
//! interface tells whether what it keeps was read for the pause a run stands at. The key
//! is worked out here alone, from the state the following holds, again only once the
//! run's head moved, so a parked run looked at many times costs no more; what was worked
//! out of a run a refollow lets go is forgotten, so a run held again is asked again. A key the following has not read is read on a task
//! of its own, off the look's path: a decision from the run's manifest, directory and
//! that state; a round from its events too, read through the following's reader. What
//! it read is handed with its key while the key is still the run's, and dropped once the
//! key changed, so an interface keeps by the key what a person typed and works none out.
//!
//! A run that waits on none is handed `Awaited(None)` once, and so is a read that finds
//! nothing waiting where the log said something does: it was answered elsewhere, and the
//! next batch says so. A read that fails, as a document a decision shows that fails
//! today's shape, is handed as `AwaitedUnread` with its key and why, and the following
//! goes on.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use yunta_core::events::DecisionOn;
use yunta_core::{Manifest, NodeId, RunId, Seq};
use yunta_replay::RunState;
use yunta_storage::LogReader;

use super::{FollowedRun, Following, RunNews};
use crate::{waiting_round, Awaited, Error, Failure, WaitingDecision, WaitingQuestions};

/// What the run read whole waits on a person for, as far as the following worked it out
/// and read it. Not [`Awaited`], what a read hands of it.
pub(super) struct Asking {
    /// The run and the head the key was last worked out at.
    worked_out_at: Option<(RunId, Option<Seq>)>,
    /// What the run asks, by the pause it waits at, as last worked out.
    key: Option<AnswerablePause>,
    /// Whether what was last handed of the run says it waits: an `Awaited(None)` is owed
    /// once it waits on none.
    waits: bool,
    /// How many times the key changed: a read hands what it read only while it is still
    /// the last one asked for.
    turn: u64,
    /// The read under way, stopped once its key changed.
    reading: Option<JoinHandle<()>>,
    /// Where a read hands what it read, with the turn it was asked for at.
    hand: mpsc::UnboundedSender<(u64, RunNews)>,
    /// What reads handed, which the following listens on beside its looks.
    pub(super) read: mpsc::UnboundedReceiver<(u64, RunNews)>,
}

/// One pause of a run that a person can answer: the decision or the round of questions
/// the run stands parked on, which the follower alone works out and hands with what it
/// read of it. Two are equal when they name the same run stopped at the same pause: a
/// decision by what it decides on and the `run_paused` the run stands at, so each time
/// the run parks on a decision is a pause of its own; a round by its node and the
/// `questions_asked` that opened it, so a run parked again on the same round, as when the
/// answers given were refused, keeps its key. Worked out again, as when the run is followed
/// anew, the same pause gives the same key. Not
/// [`PauseReason`](yunta_core::events::PauseReason), why a run stands, nor [`Awaited`],
/// what a person is asked at the pause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerablePause {
    run: RunId,
    on: AskedOn,
}

/// What a run is parked on a person for. Not [`Awaited`], what a person is shown of it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AskedOn {
    /// The decision `on` the run waits on at the pause written at `paused_at`.
    Decision { on: DecisionOn, paused_at: Seq },
    /// The round of questions `node` opened with the `questions_asked` written at
    /// `asked_at`.
    Round { node: NodeId, asked_at: Seq },
}

impl AnswerablePause {
    /// Where the log holds the event this pause was read for: the `run_paused` that
    /// parked the run on a decision, or the `questions_asked` that opened a round of
    /// questions. Two pauses of one run are one pause when they name the same event.
    pub fn seq(&self) -> Seq {
        match &self.on {
            AskedOn::Decision { paused_at, .. } => *paused_at,
            AskedOn::Round { asked_at, .. } => *asked_at,
        }
    }
}

impl Asking {
    /// Nothing worked out yet.
    pub(super) fn new() -> Asking {
        let (hand, read) = mpsc::unbounded_channel();
        Asking {
            worked_out_at: None,
            key: None,
            waits: false,
            turn: 0,
            reading: None,
            hand,
            read,
        }
    }

    /// What was worked out and read of the run a refollow lets go, forgotten with the
    /// read of it under way: a run held again is asked again, as one held for the first
    /// time is.
    pub(super) fn forget(&mut self) {
        self.worked_out_at = None;
        self.key = None;
        self.waits = false;
        self.next_turn();
    }

    /// Another turn: the read under way stopped, and what any read asked for before it
    /// hands dropped.
    fn next_turn(&mut self) {
        self.turn += 1;
        if let Some(reading) = self.reading.take() {
            reading.abort();
        }
    }
}

impl Drop for Asking {
    fn drop(&mut self) {
        if let Some(reading) = self.reading.take() {
            reading.abort();
        }
    }
}

impl Following {
    /// What the run held asks, worked out again once its head moved or another run is
    /// held, its frame taken at `now`: a key new to the following read on a task of its
    /// own, and `Awaited(None)` handed once the run waits on none after it was handed
    /// something it waits on.
    pub(super) fn ask(&mut self, now: DateTime<Utc>) {
        let at = self
            .held
            .as_ref()
            .map(|held| (held.run.run.clone(), held.run.state.head()));
        if at == self.asking.worked_out_at {
            return;
        }
        let held_before = self.asking.worked_out_at.take().map(|(run, _)| run);
        if held_before != at.as_ref().map(|(run, _)| run.clone()) {
            // Whoever follows takes another run whole, waiting on nothing yet.
            self.asking.waits = false;
        }
        self.asking.worked_out_at = at;
        let key = self.held.as_ref().and_then(|held| {
            let on = asked_of(&held.run, now)?;
            let run = held.run.run.clone();
            Some(AnswerablePause { run, on })
        });
        if key == self.asking.key {
            return;
        }
        self.asking.key = key.clone();
        self.asking.next_turn();
        match key {
            Some(key) => self.read_asked(key),
            None if self.asking.waits => {
                self.asking.waits = false;
                self.found(RunNews::Awaited(None));
            }
            None => {}
        }
    }

    /// What a read asked for at `turn` handed, handed on while its key is still the run's.
    pub(super) fn asked_read(&mut self, turn: u64, news: RunNews) {
        if turn != self.asking.turn {
            return;
        }
        self.asking.reading = None;
        self.asking.waits = !matches!(news, RunNews::Awaited(None));
        self.found(news);
    }

    /// `key` read on a task of its own, from the run held as the following holds it, and
    /// handed with what was read of it.
    fn read_asked(&mut self, key: AnswerablePause) {
        let Some(held) = &self.held else {
            return;
        };
        let read = Read {
            manifest: held.run.manifest.clone(),
            dir: held.registry.run_dir().to_path_buf(),
            state: held.run.state.clone(),
            reader: Arc::clone(&self.reader),
        };
        let (turn, hand) = (self.asking.turn, self.asking.hand.clone());
        let reading = tokio::spawn(async move {
            let news = match read.of(&key).await {
                Ok(awaited) => RunNews::Awaited(awaited.map(|awaited| (key, awaited))),
                Err(error) => RunNews::AwaitedUnread {
                    pause: key,
                    why: yunta_core::describe(&error),
                },
            };
            // best-effort: a following gone before the read came back wants it no more.
            let _ = hand.send((turn, news));
        });
        self.asking.reading = Some(reading);
    }
}

/// What a read of what a run asks reads from: the run as the following holds it, and
/// the following's reader for its events.
struct Read {
    manifest: Manifest,
    dir: PathBuf,
    state: RunState,
    reader: Arc<LogReader>,
}

impl Read {
    /// What `key` asks a person, read whole: none when nothing waits on one.
    async fn of(self, key: &AnswerablePause) -> Result<Option<Awaited>, Error> {
        match &key.on {
            AskedOn::Decision { .. } => {
                let decision = WaitingDecision::read(&self.manifest, &self.dir, &self.state);
                let decision = decision.await?;
                Ok(decision.map(|decision| Awaited::Decision(Box::new(decision))))
            }
            AskedOn::Round { node, .. } => {
                let events = self.reader.events(&key.run).await.map_err(Failure::new)?;
                let round = WaitingQuestions::read(&self.dir, &events, node.clone()).await?;
                Ok(round.map(|round| Awaited::Round(Box::new(round))))
            }
        }
    }
}

/// What `run` waits on a person for, by the pause it waits at, its frame taken at `now`:
/// none while it is not parked on a person.
fn asked_of(run: &FollowedRun, now: DateTime<Utc>) -> Option<AskedOn> {
    let (_, paused_at) = run.state.run.paused()?;
    let frame = run.frame(now);
    frame.phase.waiting_on()?;
    if let Some(node) = waiting_round(&frame) {
        let round = run.state.gates.get(node)?.rounds.last()?;
        let (node, asked_at) = (node.clone(), round.asked_at);
        return Some(AskedOn::Round { node, asked_at });
    }
    let on = yunta_engine::awaits_decision(&run.manifest, &run.state)?;
    Some(AskedOn::Decision { on, paused_at })
}
