//! What a run-level event says happened, read as a person reads it.

use crate::events::{
    BaselineOrigin, Evidence, PauseReason, PromotionCause, ResumePolicy, RunEvent, TerminalState,
    TokenUsage,
};
use crate::ModeName;

/// One thing that happened to the run itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Happening {
    Created {
        mode: ModeName,
        base_branch: String,
    },
    /// Why the engine stopped the run, as the fact its log records.
    Paused {
        pause: PauseReason,
    },
    /// The policies a resume settled its orphans under. A resume that
    /// found none names none; one that found orphans disagreeing about
    /// what to do names each, because there is no single answer to give.
    Resumed {
        policies: Vec<ResumePolicy>,
    },
    /// Whose measurement the run holds, and so whether this run ran the
    /// suite or was born holding what its lineage's root ran — and, when
    /// the suite was already failing, the code it exited with, since then
    /// no comparison against it can find anything.
    BaselineCaptured {
        origin: BaselineOrigin,
        red: Option<i32>,
    },
    /// The same, for a command `baseline.held` names: whether it holds
    /// the run's tasks.
    BaselineHeld {
        name: crate::ids::CommandName,
        origin: BaselineOrigin,
        red: Option<i32>,
    },
    /// The run moved into a checkout that already built where it stands.
    Moved {
        checkout: std::path::PathBuf,
    },
    /// The host slept for `slept` while the run was open.
    HostSuspended {
        slept: std::time::Duration,
    },
    Closed {
        terminal: TerminalState,
        tokens: TokenUsage,
    },
    PromotionSignaled {
        to: ModeName,
        cause: Option<PromotionCause>,
        legacy_reason: Option<String>,
        evidence: Evidence,
    },
}

impl From<&RunEvent> for Happening {
    fn from(event: &RunEvent) -> Self {
        match event {
            RunEvent::Created(p) => Happening::Created {
                mode: p.mode.clone(),
                base_branch: p.base_branch.clone(),
            },
            RunEvent::Paused(p) => Happening::Paused {
                pause: p.pause.clone(),
            },
            RunEvent::Resumed(p) => Happening::Resumed {
                policies: p.policies.clone(),
            },
            RunEvent::Moved(p) => Happening::Moved {
                checkout: p.checkout.clone(),
            },
            RunEvent::Finished(p) => Happening::Closed {
                terminal: p.terminal_state,
                tokens: p.metrics.tokens,
            },
            RunEvent::BaselineCaptured(p) => Happening::BaselineCaptured {
                origin: p.origin.clone(),
                red: (!p.passed()).then_some(p.results.exit_code),
            },
            RunEvent::BaselineHeld(p) => Happening::BaselineHeld {
                name: p.name.clone(),
                origin: p.origin.clone(),
                red: (!p.passed()).then_some(p.results.exit_code),
            },
            RunEvent::HostSuspended(p) => Happening::HostSuspended {
                slept: p.duration(),
            },
            RunEvent::PromotionSignaled(p) => Happening::PromotionSignaled {
                to: p.suggested_mode.clone(),
                cause: p.cause.clone(),
                legacy_reason: p.legacy_reason().map(str::to_string),
                evidence: p.evidence.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{PromotionSignaledPayload, RunPausedPayload};

    /// A pause an older writer stored as a sentence reads as the recorded
    /// pause, with the sentence as written.
    #[test]
    fn a_legacy_pause_sentence_reads_as_the_recorded_pause() {
        let legacy = serde_json::json!({ "reason": "waiting" });
        let read: RunPausedPayload = serde_json::from_value(legacy).unwrap();
        let recorded = PauseReason::Recorded {
            sentence: "waiting".to_string(),
        };
        assert_eq!(
            Happening::from(&RunEvent::Paused(read)),
            Happening::Paused { pause: recorded }
        );
    }

    /// A promotion an older writer stored as a sentence reads with no
    /// cause, and keeps the sentence as written.
    #[test]
    fn a_legacy_promotion_sentence_reads_with_no_cause() {
        let legacy = serde_json::json!({
            "reason": "exhausted",
            "evidence": [],
            "suggested_mode": "beta",
        });
        let read: PromotionSignaledPayload = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            Happening::from(&RunEvent::PromotionSignaled(read)),
            Happening::PromotionSignaled {
                to: ModeName::from_static("beta"),
                cause: None,
                legacy_reason: Some("exhausted".to_string()),
                evidence: Evidence::none(),
            }
        );
    }
}
