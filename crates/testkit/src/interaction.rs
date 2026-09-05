//! `HumanInteraction` doubles for driving a gate to a scripted decision.

use std::sync::Mutex;

use async_trait::async_trait;
use yunta_core::events::{GateWaitingPayload, HumanChoice};
use yunta_core::{OptionId, Responder};
use yunta_engine::HumanInteraction;

/// Resolves every gate with one fixed choice, recording the option ids it
/// was shown on each call so a test can assert what the engine offered.
pub struct ScriptedInteraction {
    choice: HumanChoice,
    seen_options: Mutex<Vec<Vec<OptionId>>>,
}

impl ScriptedInteraction {
    /// Builds a double that answers every gate with `choice`.
    pub fn new(choice: HumanChoice) -> Self {
        Self {
            choice,
            seen_options: Mutex::new(Vec::new()),
        }
    }

    /// Picks `option_id` with no free text — the common case of choosing a
    /// menu option and nothing else.
    pub fn choose(option_id: &str) -> Self {
        Self::new(HumanChoice {
            option: option_id.into(),
            by: "test".into(),
            free_text: None,
        })
    }

    /// The option-id lists the engine presented, oldest call first.
    pub fn seen_options(&self) -> Vec<Vec<OptionId>> {
        self.seen_options
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl HumanInteraction for ScriptedInteraction {
    async fn resolve(&self, escalation: &GateWaitingPayload) -> Option<HumanChoice> {
        self.seen_options
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(escalation.options.iter().map(|o| o.id.clone()).collect());
        Some(self.choice.clone())
    }
}

/// Approves every gate by choosing its first offered option, attributed to
/// `by`. The double for a workflow whose gates only ever need a yes.
pub struct ApproveEverything {
    by: Responder,
}

impl ApproveEverything {
    /// Approves as `by`, the responder every choice is attributed to.
    pub fn new(by: &str) -> Self {
        Self { by: by.into() }
    }
}

#[async_trait]
impl HumanInteraction for ApproveEverything {
    async fn resolve(&self, escalation: &GateWaitingPayload) -> Option<HumanChoice> {
        escalation.options.first().map(|first| HumanChoice {
            option: first.id.clone(),
            by: self.by.clone(),
            free_text: None,
        })
    }
}
