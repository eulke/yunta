//! An option a gate does not offer, and why: what the person deciding
//! is told they cannot choose, so the menu says what stands against it
//! rather than leaving it out in silence.

use serde::{Deserialize, Serialize};

use super::escalation::{Escalation, Refusal};
use super::payloads::GateWaitingPayload;
use crate::events::LoggedWords;
use crate::ids::OptionId;
use crate::shown::Flaw;
use crate::NonEmpty;

/// An option the gate withholds, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Withheld {
    pub option: OptionId,
    pub because: WhyWithheld,
}

/// Why an option is withheld: the flaws of the plan it would take the run
/// past, or the words an older log stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum WhyWithheld {
    Unprovable { flaws: NonEmpty<Flaw> },
    Logged(LoggedWords),
}

impl Escalation {
    /// The same escalation, without the options `withheld` names: each leaves
    /// the menu and is kept with its reason. An option the menu never offered
    /// is ignored.
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
                withheld: gone.clone(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AuthorGate, Claim, GateOption, HumanChoice, Leads, Menu};
    use crate::ids::{NodeId, TaskId};

    /// An author's option that takes the run on past the gate.
    fn option(id: &str) -> GateOption {
        GateOption::leading(
            id.into(),
            Leads::On {
                next: Vec::new(),
                settles: false,
            },
        )
    }

    fn approve_or_abort() -> Escalation {
        Escalation::new(
            Claim::NeedsDecision {
                node: NodeId::from_static("approve"),
            },
            Menu::Gate(AuthorGate {
                assignee: "lead".into(),
            }),
            NonEmpty::from((option("approve"), vec![option("abort")])),
        )
    }

    /// Withheld for a plan whose one flaw is a hollow test.
    fn withheld(option: &str) -> Withheld {
        Withheld {
            option: option.into(),
            because: WhyWithheld::Unprovable {
                flaws: NonEmpty::from((
                    Flaw::HollowSpecTest {
                        task: TaskId::from_static("T001"),
                        cmd: "test -f a.txt".to_string(),
                    },
                    Vec::new(),
                )),
            },
        }
    }

    #[test]
    fn a_withheld_option_leaves_the_menu_and_is_refused_with_its_reason() {
        let escalation = approve_or_abort().withholding(vec![withheld("approve")]);

        assert!(!escalation.offers(&"approve".into()));
        assert!(escalation.offers(&"abort".into()));
        assert_eq!(escalation.withheld(), [withheld("approve")]);
        assert!(
            escalation.evidence().facts().is_empty(),
            "withholding adds no fact to the record"
        );
        assert_eq!(
            escalation.accepts(&HumanChoice {
                option: "approve".into(),
                by: "lead".into(),
                free_text: None,
            }),
            Err(Refusal::Withheld {
                withheld: withheld("approve"),
            })
        );
    }

    #[test]
    fn withholding_an_option_the_menu_never_offered_changes_nothing() {
        let escalation = approve_or_abort().withholding(vec![withheld("ship")]);

        assert_eq!(escalation, approve_or_abort());
    }

    #[test]
    fn a_log_that_stored_the_reason_as_words_reads_it_back() {
        let stored: Withheld = serde_json::from_value(serde_json::json!({
            "option": "approve",
            "because": "the plan cannot be proven as it is written",
        }))
        .unwrap();
        assert_eq!(
            stored.because,
            WhyWithheld::Logged("the plan cannot be proven as it is written".into())
        );
    }
}
