//! What a run-level event says happened, read as a person reads it.

use crate::events::{BaselineOrigin, Evidence, ResumePolicy, RunEvent, TerminalState, TokenUsage};
use crate::ModeName;

/// One thing that happened to the run itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Happening {
    Created {
        mode: ModeName,
        base_branch: String,
    },
    /// The sentence the engine wrote when it stopped the run. The log
    /// persists the reason as prose rather than as the enum that built
    /// it, so this is what a reader of the log has.
    Paused {
        reason: String,
    },
    /// The policies a resume settled its orphans under. A resume that
    /// found none names none; one that found orphans disagreeing about
    /// what to do names each, because there is no single answer to give.
    Resumed {
        policies: Vec<ResumePolicy>,
    },
    /// Whose measurement the run holds, and so whether this run ran the
    /// suite or was born holding what its lineage's root ran.
    BaselineCaptured(BaselineOrigin),
    Closed {
        terminal: TerminalState,
        tokens: TokenUsage,
    },
    PromotionSignaled {
        to: ModeName,
        reason: String,
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
                reason: p.reason().to_string(),
            },
            RunEvent::Resumed(p) => Happening::Resumed {
                policies: p.policies.clone(),
            },
            RunEvent::Finished(p) => Happening::Closed {
                terminal: p.terminal_state,
                tokens: p.metrics.tokens,
            },
            RunEvent::BaselineCaptured(p) => Happening::BaselineCaptured(p.origin.clone()),
            RunEvent::PromotionSignaled(p) => Happening::PromotionSignaled {
                to: p.suggested_mode.clone(),
                reason: p.reason.clone(),
                evidence: p.evidence.clone(),
            },
        }
    }
}
