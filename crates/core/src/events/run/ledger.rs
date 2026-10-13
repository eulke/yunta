//! What the run itself is doing, folded from its own events.
//!
//! Every surface that reports where a run stands — the status page, the
//! listing's row, the exit code, the receipt — reads it from here rather
//! than walking the log for a `run_finished` itself. With four kinds
//! that move a run between phases, a second fold is a second answer.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::events::meta::EventMeta;
use crate::events::run::kinds::RunEvent;
use crate::events::run::metrics::RunMetrics;
use crate::events::run::suspensions::{Suspension, Suspensions};
use crate::events::{
    BaselineCapturedPayload, BaselineHeldPayload, BaselineOrigin, EnvironmentDrift, Evidence,
    ExecutionEnvironment, PauseReason, PromotionCause, RunCreatedPayload, RunFinishedPayload,
    RunResumedPayload, TerminalState,
};
use crate::ids::{ModeName, Seq};
use crate::TreeId;

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

/// A promotion the run signalled: the mode it asked for, what asked for
/// it, and the record it rests on.
#[derive(Debug, Clone, PartialEq)]
pub struct Promotion {
    pub to: ModeName,
    pub cause: Option<PromotionCause>,
    /// The sentence an older log stored in place of `cause`.
    pub legacy_reason: Option<String>,
    pub evidence: Evidence,
    pub at: Seq,
}

/// How the run closed, as the `run_finished` that closed it recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    pub terminal: TerminalState,
    pub seq: Seq,
    pub at: DateTime<Utc>,
    pub metrics: RunMetrics,
    pub closed_by: Option<crate::Responder>,
}

impl Finished {
    /// The close `payload` records, placed by its envelope.
    fn of(payload: &RunFinishedPayload, meta: &EventMeta<'_>) -> Self {
        Finished {
            terminal: payload.terminal_state,
            seq: meta.seq,
            at: meta.at,
            metrics: payload.metrics.clone(),
            closed_by: payload.closed_by.clone(),
        }
    }
}

/// The run's own events, folded.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunLedger {
    phase: RunPhaseRaw,
    mode: ModeName,
    left_out: Vec<crate::LeftOut>,
    /// The last close the log records for this run.
    finished: Option<Finished>,
    paused: Option<(PauseReason, Seq)>,
    resumed_after: Option<Seq>,
    promotion: Option<Promotion>,
    /// The measurement this run holds: its own, or the one it was born
    /// holding. `None` for a lineage whose root declared no suite.
    baseline: Option<BaselineCapturedPayload>,
    /// What each command `baseline.held` names did beside the suite, by
    /// name.
    held: BTreeMap<crate::ids::CommandName, BaselineHeldPayload>,
    /// Every span the host was suspended while the run was open.
    suspensions: Suspensions,
    /// Whether an invocation has woken this run. A birth writes any
    /// number of events — what the run is, what it holds, the
    /// measurement it was handed — and none of them is a wake.
    woken: bool,
    /// Every invocation of the run — its birth and each wake — from the
    /// seq it began at, with what its commands ran with when it recorded
    /// that.
    invocations: Vec<(Seq, Option<ExecutionEnvironment>)>,
    /// Whether the run was born in a checkout holding exactly the commit
    /// it opened on.
    opens_on_base: bool,
    /// The checkout of its own the run works in, as its log last named it.
    checkout: Option<std::path::PathBuf>,
    /// The wake that last moved the run to another checkout.
    moved_at: Option<Seq>,
    /// Where the run last took the checkout it works in: its birth, a
    /// wake that moved it, or a move between two of its steps.
    bound_at: Option<Seq>,
    /// The tree the latest wake found the run at, with the seq of that
    /// wake. `None` when the latest wake found none.
    woke_on: Option<(Seq, TreeId)>,
    /// When the latest wake was logged, whether or not it found a tree.
    resumed_at: Option<DateTime<Utc>>,
}

impl RunLedger {
    /// The checkout of its own the run works in, as its log names it:
    /// where it was born, or where a later wake moved it. `None` for a run
    /// working in a person's checkout, or one whose log never named it.
    pub fn checkout(&self) -> Option<&std::path::Path> {
        self.checkout.as_deref()
    }

    /// Where the run last took the checkout it works in — `None` for a
    /// run working in a person's checkout.
    pub fn bound_at(&self) -> Option<Seq> {
        self.bound_at
    }

    /// The tree the latest wake found the run at, with the seq of that
    /// wake. `None` when the latest wake found no tree.
    pub fn woke_on(&self) -> Option<(Seq, &TreeId)> {
        self.woke_on.as_ref().map(|(seq, tree)| (*seq, tree))
    }

    /// When the latest wake was logged, whether or not it found a tree.
    pub fn resumed_at(&self) -> Option<DateTime<Utc>> {
        self.resumed_at
    }

    /// Whether the latest wake moved the run to another checkout than the
    /// one it worked in before: a session started there cannot be picked
    /// back up from here.
    pub fn moved_on_waking(&self) -> bool {
        self.moved_at.is_some() && self.moved_at == self.resumed_after
    }

    /// How the environment the run's commands run with changed since
    /// the run was born, as of its latest wake. `None` when it did not,
    /// or when the log does not say.
    pub fn environment_drift(&self) -> Option<EnvironmentDrift> {
        let (born, wakes) = self.invocations.split_first()?;
        let woken = wakes
            .iter()
            .rev()
            .find_map(|(_, in_force)| in_force.as_ref())?;
        born.1.as_ref()?.drift_to(woken)
    }

    /// What the run's commands ran with at `seq`: the environment the
    /// invocation that wrote it recorded. `None` before the run was born,
    /// or when that invocation recorded none.
    pub fn environment_at(&self, seq: Seq) -> Option<&ExecutionEnvironment> {
        self.invocations
            .iter()
            .rev()
            .find(|(began, _)| *began <= seq)
            .and_then(|(_, in_force)| in_force.as_ref())
    }

    /// Whether the run was born in a checkout of its own holding exactly
    /// the commit it opened on: what lets its suite be measured aside.
    pub fn opens_on_base(&self) -> bool {
        self.opens_on_base
    }

    /// What the run's commands run with in its latest invocation.
    pub fn environment_now(&self) -> Option<&ExecutionEnvironment> {
        self.invocations
            .last()
            .and_then(|(_, in_force)| in_force.as_ref())
    }

    /// The measurement this run holds — its own, or the one it was born
    /// holding. `None` for a lineage whose root declared no suite.
    pub fn baseline(&self) -> Option<&BaselineCapturedPayload> {
        self.baseline.as_ref()
    }

    /// What each held command did beside the suite, by name.
    pub fn held(&self) -> &BTreeMap<crate::ids::CommandName, BaselineHeldPayload> {
        &self.held
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

    /// How the run closed, if it did.
    pub fn finished(&self) -> Option<&Finished> {
        self.finished.as_ref()
    }

    /// Why the run is parked, when it is: a `run_paused` with no
    /// `run_resumed` after it.
    pub fn paused(&self) -> Option<(&PauseReason, Seq)> {
        self.paused
            .as_ref()
            .filter(|(_, at)| self.resumed_after.is_none_or(|resumed| resumed < *at))
            .map(|(pause, at)| (pause, *at))
    }

    /// The latest `run_paused` with its reason, whether or not a resume came
    /// after it: the pause the run last took, which `paused` drops once a
    /// resume follows it.
    pub fn last_pause(&self) -> Option<(&PauseReason, Seq)> {
        self.paused.as_ref().map(|(pause, at)| (pause, *at))
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
        match event {
            RunEvent::Created(p) => self.created(p, meta),
            RunEvent::Paused(p) => {
                self.woken = true;
                self.phase = RunPhaseRaw::Paused;
                self.paused = Some((p.pause.clone(), meta.seq));
            }
            RunEvent::Resumed(p) => self.resumed(p, meta),
            RunEvent::Moved(p) => {
                self.checkout = Some(p.checkout.clone());
                self.bound_at = Some(meta.seq);
            }
            RunEvent::Finished(p) => {
                self.phase = RunPhaseRaw::Closed;
                self.finished = Some(Finished::of(p, meta));
            }
            RunEvent::BaselineCaptured(p) => {
                // Measuring is something an invocation does; being born
                // holding a measurement is not.
                self.woken |= p.origin == BaselineOrigin::Measured;
                self.baseline = Some(p.clone());
            }
            RunEvent::BaselineHeld(p) => {
                self.woken |= p.origin == BaselineOrigin::Measured;
                self.held.insert(p.name.clone(), p.clone());
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
                    cause: p.cause.clone(),
                    legacy_reason: p.legacy_reason().map(str::to_string),
                    evidence: p.evidence.clone(),
                    at: meta.seq,
                });
            }
        }
    }

    /// The run's birth: what it is, where it works and the environment its
    /// first invocation ran with.
    fn created(&mut self, p: &RunCreatedPayload, meta: &EventMeta<'_>) {
        self.phase = RunPhaseRaw::Open;
        self.mode = p.mode.clone();
        self.left_out = p.left_out.clone();
        self.opens_on_base = p.opens_on_base;
        self.checkout = p.checkout.clone();
        self.bound_at = p.checkout.as_ref().map(|_| meta.seq);
        self.invocations
            .push((meta.seq, p.environment.as_deref().cloned()));
    }

    /// A wake: the run works again under the invocation that woke it, and
    /// moves to another checkout when the wake names one.
    fn resumed(&mut self, p: &RunResumedPayload, meta: &EventMeta<'_>) {
        self.woken = true;
        self.phase = RunPhaseRaw::Open;
        self.resumed_after = Some(meta.seq);
        self.resumed_at = Some(meta.at);
        self.woke_on = p.tree.clone().map(|tree| (meta.seq, tree));
        self.invocations.push((meta.seq, p.environment.clone()));
        if let Some(checkout) = &p.checkout {
            self.checkout = Some(checkout.clone());
            self.moved_at = Some(meta.seq);
            self.bound_at = Some(meta.seq);
        }
    }
}

#[cfg(test)]
mod tests;
