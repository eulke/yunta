//! See [`super`]. `doctor --session`: how one binding's session ended, and
//! the row a person reads it as, under the binding and the runners that name
//! it.

use surco::components::{Check, Found};
use surco::words::Standing;
use yunta_core::events::{Failure, PauseReason, SessionDeath};
use yunta_core::{NodeId, RunnerCandidate, RunnerName};

/// A runner that names a binding, and where among its candidates. Not an order a run falls
/// back along: a run takes a runner's first candidate this binary builds, and a later one
/// serves a run pinned to its adapter, or a binary that builds none of those before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedBy {
    pub runner: RunnerName,
    /// Its place among the runner's candidates, counted from zero: the first is 0.
    pub position: usize,
}

impl NamedBy {
    /// The runner as a session's row names it: alone where the binding is its first
    /// candidate, `reviewer's 2nd candidate` where it is a later one.
    fn said(&self) -> String {
        match self.position {
            0 => self.runner.to_string(),
            at => format!(
                "{}'s {} candidate",
                self.runner,
                surco::text::ordinal(at + 1)
            ),
        }
    }
}

/// How a binding's session ended.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionOutcome {
    /// It opened, ran and closed its turn, spending this many tokens.
    Ok { tokens: u64 },
    /// It ended without ever reporting a terminal event.
    Died(Box<SessionDeath>),
    /// It opened, and did not hand over the questions document: the failure
    /// the node met, or none when the run ended without one accepted.
    NoDelivery { failure: Option<Failure> },
    /// The node failed for its own reasons.
    Failed { failure: Failure },
    /// The run ended, as `ended` says, without a complete accepted
    /// submission.
    Incomplete { ended: ProbeEnded },
    /// The probe run could not be built at all: what stopped it, in its own
    /// words.
    Unbuilt { said: String },
}

/// How a probe run ended without a complete accepted submission.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeEnded {
    Finished,
    /// It parked, for `reason`.
    Paused {
        reason: PauseReason,
    },
    /// Node `node` failed it.
    Failed {
        node: NodeId,
        failure: Failure,
    },
}

impl ProbeEnded {
    /// How it ended, as the rest of the row reads.
    fn said(&self) -> String {
        match self {
            ProbeEnded::Finished => Standing::Finished.word().to_string(),
            ProbeEnded::Paused { reason } => format!(
                "{} ({})",
                crate::pause::standing(reason),
                crate::pause::sentence(reason)
            ),
            ProbeEnded::Failed { node, failure } => {
                let said = PauseReason::NodeFailed {
                    node: node.clone(),
                    failure: failure.clone(),
                };
                format!("{} ({})", Standing::Failed, crate::pause::sentence(&said))
            }
        }
    }
}

/// One binding's session, as its row: the binding and the runners that name
/// it, and how its session ended.
pub(super) fn session_check(
    candidate: &RunnerCandidate,
    named_by: &[NamedBy],
    outcome: &SessionOutcome,
) -> Check {
    let (found, said) = match outcome {
        SessionOutcome::Ok { tokens } => (
            Found::Holds,
            format!("ok: {}", surco::figures::Tokens(*tokens)),
        ),
        SessionOutcome::Died(died) => (
            Found::Problem,
            format!("session died: {}", crate::failure::session_death(died)),
        ),
        SessionOutcome::NoDelivery { failure } => (
            Found::Problem,
            format!(
                "session opened, no questions document: {}",
                failure.as_ref().map_or_else(
                    || "the run ended without artifact_accepted".to_string(),
                    crate::failure::sentence
                )
            ),
        ),
        SessionOutcome::Failed { failure } => (
            Found::Problem,
            format!("probe failed: {}", crate::failure::sentence(failure)),
        ),
        SessionOutcome::Incomplete { ended } => (
            Found::Problem,
            format!(
                "probe failed: the run ended {} without a complete accepted submission",
                ended.said()
            ),
        ),
        SessionOutcome::Unbuilt { said } => (Found::Problem, format!("probe failed: {said}")),
    };
    let binding = label(candidate);
    let runners = named_by
        .iter()
        .map(NamedBy::said)
        .collect::<Vec<_>>()
        .join(", ");
    Check::new(found, format!("{binding} ({runners})"), said).brief(binding)
}

/// The candidate as its `runners:` line names it ([`crate::runner::candidate`]), then
/// `/agent` where it names one: what a reader matches back to a line of their own.
fn label(candidate: &RunnerCandidate) -> String {
    let named = crate::runner::candidate(&candidate.adapter, candidate.model.as_ref());
    match &candidate.agent {
        Some(agent) => format!("{named}/{agent}"),
        None => named,
    }
}
