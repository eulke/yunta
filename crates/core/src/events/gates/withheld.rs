//! An option a gate does not offer, and why: what the person deciding
//! is told they cannot choose, so the menu says what stands against it
//! rather than leaving it out in silence.

use serde::{Deserialize, Serialize};

use super::payloads::{Escalation, GateWaitingPayload, Refusal};
use crate::events::Fact;
use crate::ids::OptionId;

/// An option the gate withholds, and the reason it gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Withheld {
    pub option: OptionId,
    pub because: String,
}

impl Escalation {
    /// The same escalation, without the options `withheld` names: each
    /// leaves the menu, is recorded with its reason, and the reason is a
    /// fact of the record, so every surface that prints the escalation
    /// says why. An option the menu never offered is ignored.
    pub fn withholding(mut self, withheld: Vec<Withheld>) -> Self {
        let payload: &mut GateWaitingPayload = &mut self.0;
        for gone in withheld {
            if !payload
                .options
                .iter()
                .any(|option| option.id == gone.option)
            {
                continue;
            }
            payload.options.retain(|option| option.id != gone.option);
            payload.evidence.push(Fact::labelled(
                "withheld",
                format!("`{}` — {}", gone.option, gone.because),
            ));
            payload.withheld.push(gone);
        }
        self
    }
}

impl GateWaitingPayload {
    /// The options the gate does not offer, each with its reason.
    pub fn withheld(&self) -> &[Withheld] {
        &self.withheld
    }

    /// Why `option` is refused as one the gate withholds, if it is.
    pub(super) fn refused_as_withheld(&self, option: &OptionId) -> Option<Refusal> {
        self.withheld
            .iter()
            .find(|gone| &gone.option == option)
            .map(|gone| Refusal::Withheld {
                chosen: gone.option.clone(),
                because: gone.because.clone(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Evidence, GateOption, HumanChoice};
    use crate::NonEmpty;

    fn option(id: &str) -> GateOption {
        GateOption {
            id: id.into(),
            label: id.to_string(),
            tradeoff: String::new(),
            asks: None,
        }
    }

    fn approve_or_abort() -> Escalation {
        Escalation::new(
            "Approve?",
            Evidence::Prose("assignee: lead".to_string()),
            NonEmpty::from((option("approve"), vec![option("abort")])),
        )
        .unwrap()
    }

    fn withheld(option: &str) -> Withheld {
        Withheld {
            option: option.into(),
            because: "the plan cannot be proven".to_string(),
        }
    }

    #[test]
    fn a_withheld_option_leaves_the_menu_and_its_reason_joins_the_record() {
        let escalation = approve_or_abort().withholding(vec![withheld("approve")]);

        assert!(!escalation.offers(&"approve".into()));
        assert!(escalation.offers(&"abort".into()));
        assert_eq!(escalation.withheld(), [withheld("approve")]);
        assert_eq!(
            escalation.evidence().lines(),
            [
                "assignee: lead",
                "withheld: `approve` — the plan cannot be proven"
            ]
        );
        assert_eq!(
            escalation.accepts(&HumanChoice {
                option: "approve".into(),
                by: "lead".into(),
                free_text: None,
            }),
            Err(Refusal::Withheld {
                chosen: "approve".into(),
                because: "the plan cannot be proven".to_string(),
            })
        );
    }

    #[test]
    fn withholding_an_option_the_menu_never_offered_changes_nothing() {
        let escalation = approve_or_abort().withholding(vec![withheld("ship")]);

        assert_eq!(escalation, approve_or_abort());
    }
}
