//! What the run itself is doing, folded from its own events.
//!
//! Every surface that reports where a run stands — the status page, the
//! listing's row, the exit code, the receipt — reads it from here rather
//! than walking the log for a `run_finished` itself. With four kinds
//! that move a run between phases, a second fold is a second answer.

use chrono::{DateTime, Utc};

use crate::events::meta::EventMeta;
use crate::events::run::kinds::RunEvent;
use crate::events::{Evidence, TerminalState, TokenUsage};
use crate::ids::{ModeName, Seq};

/// Where the run stands, as its own events say — without reference to
/// what its nodes are doing. A surface that reports a run parked on a
/// person combines this with the node ledger; this is the run's half.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RunPhaseRaw {
    /// No `run_created` on the log yet.
    #[default]
    Unborn,
    /// Created and not closed.
    Open,
    /// A `run_paused` with no `run_resumed` after it.
    Paused,
    /// A `run_finished`.
    Closed,
}

/// A promotion the run signalled: the mode it asked for and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Promotion {
    pub to: ModeName,
    pub reason: String,
    pub evidence: Evidence,
    pub at: Seq,
}

/// The run's own events, folded.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunLedger {
    phase: RunPhaseRaw,
    mode: ModeName,
    born_at: Option<DateTime<Utc>>,
    closed: Option<(TerminalState, Seq)>,
    paused: Option<(String, Seq)>,
    resumed_after: Option<Seq>,
    promotion: Option<Promotion>,
    /// What the run's own close reported it spent. A run that has not
    /// closed reports nothing here; the running total is the node
    /// ledger's.
    closed_tokens: Option<TokenUsage>,
}

impl RunLedger {
    /// Where the run stands.
    pub fn phase(&self) -> &RunPhaseRaw {
        &self.phase
    }

    /// The mode `run_created` froze. The one place a run's mode is
    /// derived: a resume and every stats surface read the same answer.
    pub fn mode(&self) -> &ModeName {
        &self.mode
    }

    /// When the run's first event was written; `None` for a log with
    /// none.
    pub fn born_at(&self) -> Option<DateTime<Utc>> {
        self.born_at
    }

    /// How the run closed, and where.
    pub fn closed(&self) -> Option<(&TerminalState, Seq)> {
        self.closed.as_ref().map(|(state, seq)| (state, *seq))
    }

    /// What the run closed reporting it spent.
    pub fn closed_tokens(&self) -> Option<TokenUsage> {
        self.closed_tokens
    }

    /// Why the run is parked, when it is: a `run_paused` with no
    /// `run_resumed` after it.
    pub fn paused(&self) -> Option<(&str, Seq)> {
        self.paused
            .as_ref()
            .filter(|(_, at)| self.resumed_after.is_none_or(|resumed| resumed < *at))
            .map(|(reason, at)| (reason.as_str(), *at))
    }

    /// Where the latest `run_paused` sits, whether or not a resume came
    /// after it — the end of the window a decision could have been
    /// seeded into, which a later resume does not reopen.
    pub fn last_paused_at(&self) -> Option<Seq> {
        self.paused.as_ref().map(|(_, at)| *at)
    }

    /// The promotion the run signalled, if it did.
    pub fn promotion(&self) -> Option<&Promotion> {
        self.promotion.as_ref()
    }

    /// Folds one of the run's own events.
    pub fn apply(&mut self, event: &RunEvent, meta: &EventMeta<'_>) {
        if self.born_at.is_none() {
            self.born_at = Some(meta.at);
        }
        match event {
            RunEvent::Created(p) => {
                self.phase = RunPhaseRaw::Open;
                self.mode = p.mode.clone();
            }
            RunEvent::Paused(p) => {
                self.phase = RunPhaseRaw::Paused;
                self.paused = Some((p.reason().to_string(), meta.seq));
            }
            RunEvent::Resumed(_) => {
                self.phase = RunPhaseRaw::Open;
                self.resumed_after = Some(meta.seq);
            }
            RunEvent::Finished(p) => {
                self.phase = RunPhaseRaw::Closed;
                self.closed = Some((p.terminal_state, meta.seq));
                self.closed_tokens = Some(p.metrics.tokens);
            }
            RunEvent::PromotionSignaled(p) => {
                self.promotion = Some(Promotion {
                    to: p.suggested_mode.clone(),
                    reason: p.reason.clone(),
                    evidence: p.evidence.clone(),
                    at: meta.seq,
                });
            }
        }
    }
}
