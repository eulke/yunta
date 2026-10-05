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
}

impl RunLedger {
    /// The checkout of its own the run works in, as its log names it:
    /// where it was born, or where a later wake moved it. `None` for a run
    /// working in a person's checkout, or one whose log never named it.
    pub fn checkout(&self) -> Option<&std::path::Path> {
        self.checkout.as_deref()
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
                self.opens_on_base = p.opens_on_base;
                self.checkout = p.checkout.clone();
                self.invocations
                    .push((meta.seq, p.environment.as_deref().cloned()));
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
                self.invocations.push((meta.seq, p.environment.clone()));
                if let Some(checkout) = &p.checkout {
                    self.checkout = Some(checkout.clone());
                    self.moved_at = Some(meta.seq);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{RunCreatedPayload, RunResumedPayload};

    fn environment(path: &str) -> ExecutionEnvironment {
        ExecutionEnvironment {
            shell: "/bin/sh".to_string(),
            path: vec![path.to_string()],
        }
    }

    fn at(ledger: &mut RunLedger, seq: u64, event: RunEvent) {
        let meta = EventMeta {
            seq: Seq::from(seq),
            at: chrono::DateTime::UNIX_EPOCH,
            node: None,
        };
        ledger.apply(&event, &meta);
    }

    /// Each seq is answered with what the invocation that wrote it ran
    /// with: the birth's before the first wake, then each wake's own —
    /// and nothing for a wake that recorded none.
    #[test]
    fn the_environment_at_a_seq_is_the_one_its_invocation_ran_with() {
        let mut ledger = RunLedger::default();
        let created = RunCreatedPayload {
            checkout: None,
            manifest_hash: crate::sha256_hex(b"manifest"),
            inputs: Default::default(),
            mode: Default::default(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: crate::sha256_hex(b"base").as_str().into(),
            environment: Some(Box::new(environment("/born"))),
            left_out: Vec::new(),
            opens_on_base: false,
        };
        at(&mut ledger, 1, RunEvent::Created(created));
        let woken = RunResumedPayload::new(Vec::new(), Some(environment("/woken")));
        at(&mut ledger, 5, RunEvent::Resumed(woken));
        at(
            &mut ledger,
            9,
            RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None)),
        );

        let path_at = |seq: u64| {
            ledger
                .environment_at(Seq::from(seq))
                .map(|in_force| in_force.path.join(":"))
        };
        assert_eq!(path_at(4).as_deref(), Some("/born"));
        assert_eq!(path_at(5).as_deref(), Some("/woken"));
        assert_eq!(path_at(8).as_deref(), Some("/woken"));
        assert_eq!(path_at(9), None);
        assert_eq!(ledger.environment_now(), None);
    }

    fn born_in(checkout: Option<&str>) -> RunCreatedPayload {
        RunCreatedPayload {
            checkout: checkout.map(std::path::PathBuf::from),
            manifest_hash: crate::sha256_hex(b"manifest"),
            inputs: Default::default(),
            mode: Default::default(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: crate::sha256_hex(b"base").as_str().into(),
            environment: None,
            left_out: Vec::new(),
            opens_on_base: false,
        }
    }

    /// The run's checkout is the one its log last named: where it was born,
    /// unchanged by a wake that names none, and moved by one that does.
    #[test]
    fn the_checkout_is_the_one_the_log_last_named() {
        let mut ledger = RunLedger::default();
        at(
            &mut ledger,
            1,
            RunEvent::Created(born_in(Some("/pool/slot-1"))),
        );
        let still = RunResumedPayload::new(Vec::new(), None);
        at(&mut ledger, 2, RunEvent::Resumed(still));
        assert_eq!(
            ledger.checkout(),
            Some(std::path::Path::new("/pool/slot-1"))
        );

        let moved = RunResumedPayload::new(Vec::new(), None).in_checkout("/pool/slot-3".into());
        at(&mut ledger, 3, RunEvent::Resumed(moved));

        assert_eq!(
            ledger.checkout(),
            Some(std::path::Path::new("/pool/slot-3"))
        );
    }

    /// Only the wake that moved the run says so: the next one, in the same
    /// checkout, does not.
    #[test]
    fn a_wake_that_moved_the_run_says_so_and_the_next_does_not() {
        let mut ledger = RunLedger::default();
        at(
            &mut ledger,
            1,
            RunEvent::Created(born_in(Some("/pool/slot-1"))),
        );
        let moved = RunResumedPayload::new(Vec::new(), None).in_checkout("/pool/slot-2".into());
        at(&mut ledger, 2, RunEvent::Resumed(moved));
        assert!(ledger.moved_on_waking());

        at(
            &mut ledger,
            3,
            RunEvent::Resumed(RunResumedPayload::new(Vec::new(), None)),
        );

        assert!(!ledger.moved_on_waking());
    }

    /// A log written before the field names no checkout, and reads back.
    #[test]
    fn an_old_log_names_no_checkout() {
        let written = serde_json::to_value(born_in(None)).unwrap();
        assert!(written.get("checkout").is_none(), "{written}");
        let read: RunCreatedPayload = serde_json::from_value(written).unwrap();
        let mut ledger = RunLedger::default();
        at(&mut ledger, 1, RunEvent::Created(read));

        assert_eq!(ledger.checkout(), None);
    }

    /// A checkout the log names round-trips as written.
    #[test]
    fn run_created_round_trips_its_checkout() {
        let born = born_in(Some("/pool/slot-1"));
        let written = serde_json::to_string(&born).unwrap();
        let read: RunCreatedPayload = serde_json::from_str(&written).unwrap();
        assert_eq!(read, born);
    }
}
