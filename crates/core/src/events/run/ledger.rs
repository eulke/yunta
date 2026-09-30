//! What the run itself is doing, folded from its own events.
//!
//! Every surface that reports where a run stands — the status page, the
//! listing's row, the exit code, the receipt — reads it from here rather
//! than walking the log for a `run_finished` itself. With four kinds
//! that move a run between phases, a second fold is a second answer.

use chrono::{DateTime, Utc};

use crate::events::meta::EventMeta;
use crate::events::run::kinds::RunEvent;
use crate::events::run::suspensions::{Suspension, Suspensions};
use crate::events::{
    BaselineCapturedPayload, BaselineOrigin, EnvironmentDrift, Evidence, ExecutionEnvironment,
    TerminalState, TokenUsage,
};
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
    left_out: Vec<crate::LeftOut>,
    born_at: Option<DateTime<Utc>>,
    closed: Option<(TerminalState, Seq)>,
    paused: Option<(String, Seq)>,
    resumed_after: Option<Seq>,
    promotion: Option<Promotion>,
    /// What the run's own close reported it spent. A run that has not
    /// closed reports nothing here; the running total is the node
    /// ledger's.
    closed_tokens: Option<TokenUsage>,
    /// The measurement this run holds: its own, or the one it was born
    /// holding. `None` for a lineage whose root declared no suite.
    baseline: Option<BaselineCapturedPayload>,
    /// Every span the host was suspended while the run was open.
    suspensions: Suspensions,
    /// Whether an invocation has woken this run. A birth writes any
    /// number of events — what the run is, what it holds, the
    /// measurement it was handed — and none of them is a wake.
    woken: bool,
    /// What the run's commands ran with at birth, and at its latest wake
    /// that recorded one.
    born_in: Option<ExecutionEnvironment>,
    woken_in: Option<ExecutionEnvironment>,
}

impl RunLedger {
    /// How the environment the run's commands run with changed since
    /// the run was born, as of its latest wake. `None` when it did not,
    /// or when the log does not say.
    pub fn environment_drift(&self) -> Option<EnvironmentDrift> {
        self.born_in.as_ref()?.drift_to(self.woken_in.as_ref()?)
    }

    /// The measurement this run holds — its own, or the one it was born
    /// holding. `None` for a lineage whose root declared no suite.
    pub fn baseline(&self) -> Option<&BaselineCapturedPayload> {
        self.baseline.as_ref()
    }

    /// Whether the run's own events say an invocation woke it: a
    /// pause, a resume, or a measurement this run took. A birth writes
    /// run events too — what the run is, the measurement it was handed
    /// — and none of them is a wake.
    ///
    /// The whole-log answer is `RunState::woken`, which reads this
    /// beside the nodes an invocation that died without pausing left
    /// behind.
    pub fn woken(&self) -> bool {
        self.woken
    }

    /// Where the run stands.
    pub fn phase(&self) -> &RunPhaseRaw {
        &self.phase
    }

    /// The mode `run_created` froze. The one place a run's mode is
    /// derived: a resume and every stats surface read the same answer.
    pub fn mode(&self) -> &ModeName {
        &self.mode
    }

    /// The nodes `run_created` left out of this run.
    pub fn left_out(&self) -> &[crate::LeftOut] {
        &self.left_out
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

    /// Every span the host was suspended while the run was open — what a
    /// duration the run reports leaves out.
    pub fn suspensions(&self) -> &Suspensions {
        &self.suspensions
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
                self.left_out = p.left_out.clone();
                self.born_in = p.environment.as_deref().cloned();
            }
            RunEvent::Paused(p) => {
                self.woken = true;
                self.phase = RunPhaseRaw::Paused;
                self.paused = Some((p.reason().to_string(), meta.seq));
            }
            RunEvent::Resumed(p) => {
                self.woken = true;
                self.phase = RunPhaseRaw::Open;
                self.resumed_after = Some(meta.seq);
                if p.environment.is_some() {
                    self.woken_in = p.environment.clone();
                }
            }
            RunEvent::Finished(p) => {
                self.phase = RunPhaseRaw::Closed;
                self.closed = Some((p.terminal_state, meta.seq));
                self.closed_tokens = Some(p.metrics.tokens);
            }
            RunEvent::BaselineCaptured(p) => {
                // Measuring is something an invocation does; being born
                // holding a measurement is not.
                self.woken |= p.origin == BaselineOrigin::Measured;
                self.baseline = Some(p.clone());
            }
            // The machine slept: a fact about the host, not something an
            // invocation did, so it neither wakes the run nor moves it
            // between phases.
            RunEvent::HostSuspended(p) => self.suspensions.push(Suspension {
                woke_at: meta.at,
                slept: p.duration(),
            }),
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
