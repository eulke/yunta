//! An escalation on its way to a person: the claim, the record it is audited
//! against, and the menu of answers, with the reasons an answer is refused.

use serde::{Deserialize, Serialize};

use super::payloads::{DepartureMenu, FailureMenu, GateOption, GateWaitingPayload, Shown};
use crate::events::{Evidence, Fact};
use crate::ids::OptionId;
use crate::NonEmpty;

/// Why an answer does not count as a decision on an escalation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// The option is not on the menu.
    #[error("`{chosen}` is not one of the options offered ({offered})")]
    OffMenu { chosen: OptionId, offered: String },
    /// The option asks for words, and the answer has none.
    #[error("`{chosen}` asks \"{asks}\", and the answer says nothing")]
    Unsaid { chosen: OptionId, asks: String },
    /// The gate withholds the option, for the reason it gives.
    #[error("`{chosen}` is withheld: {because}")]
    Withheld { chosen: OptionId, because: String },
}

/// Why an escalation was refused before it reached anyone.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EscalationError {
    /// The claim repeats a fact the record already holds. Every surface
    /// prints the two under separate headings, so a summary that quotes
    /// its own evidence reads as the same sentence twice, the second
    /// time under a heading that promised something new. The fact is
    /// carried as data; a surface that shows it draws it.
    #[error("the summary repeats what the evidence already states")]
    SummaryRepeatsEvidence { fact: Fact },
}

/// An escalation on its way to a person: the claim, the record it is
/// audited against, and the menu of answers.
///
/// The one way a `gate_waiting` payload comes to exist: which half of the
/// story goes into the summary and which into the evidence is decided
/// here, once, and the menu cannot be empty by type, since an escalation
/// with no options refuses every answer it is given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(from = "GateWaitingPayload", into = "GateWaitingPayload")]
#[schemars(with = "GateWaitingPayload")]
pub struct Escalation(pub(super) GateWaitingPayload);

impl Escalation {
    /// One escalation, refused if its claim repeats its own record.
    pub fn new(
        summary: impl Into<String>,
        evidence: Evidence,
        options: NonEmpty<GateOption>,
    ) -> Result<Self, EscalationError> {
        let summary = summary.into();
        if let Some(fact) = evidence.facts().iter().find(|fact| repeats(&summary, fact)) {
            return Err(EscalationError::SummaryRepeatsEvidence { fact: fact.clone() });
        }
        Ok(Escalation(GateWaitingPayload {
            summary,
            evidence,
            options: options.into_vec(),
            external_ref: None,
            shows: Vec::new(),
            withheld: Vec::new(),
            scope: None,
            failure: None,
            departure: None,
        }))
    }

    /// An escalation whose answer comes from a forge: the pull request
    /// is the menu, and `external_ref` is the handle a later poll needs
    /// to find it again from a process that knows nothing but the log.
    ///
    /// No options, and that is the shape rather than an omission: no
    /// answer given here would count, because the decision is made and
    /// recorded on the forge. `offers` refusing every option is the
    /// truth about this gate, not the bug it would be on a local one.
    pub fn published_to(
        summary: impl Into<String>,
        evidence: Evidence,
        external_ref: impl Into<String>,
    ) -> Result<Self, EscalationError> {
        let summary = summary.into();
        if let Some(fact) = evidence.facts().iter().find(|fact| repeats(&summary, fact)) {
            return Err(EscalationError::SummaryRepeatsEvidence { fact: fact.clone() });
        }
        Ok(Escalation(GateWaitingPayload {
            summary,
            evidence,
            options: Vec::new(),
            external_ref: Some(external_ref.into()),
            shows: Vec::new(),
            withheld: Vec::new(),
            scope: None,
            failure: None,
            departure: None,
        }))
    }

    /// The same escalation, putting `shows` in front of the person.
    pub fn showing(mut self, shows: Vec<Shown>) -> Self {
        self.0.shows = shows;
        self
    }

    /// The same escalation, about the failure a node ended with, and the
    /// menu that failure offers.
    pub fn with_failure(mut self, menu: FailureMenu) -> Self {
        self.0.failure = Some(Box::new(menu));
        self
    }

    /// The same escalation, about a departure a task's session declared.
    pub fn with_departure(mut self, menu: DepartureMenu) -> Self {
        self.0.departure = Some(Box::new(menu));
        self
    }

    /// The payload, for the event that carries it.
    pub fn into_payload(self) -> GateWaitingPayload {
        self.0
    }
}

impl From<GateWaitingPayload> for Escalation {
    /// A payload read off the log is taken as written, without re-checking its claim.
    fn from(payload: GateWaitingPayload) -> Self {
        Escalation(payload)
    }
}

impl From<Escalation> for GateWaitingPayload {
    fn from(escalation: Escalation) -> Self {
        escalation.0
    }
}

/// Whether `summary` already states what `fact` records.
///
/// A fact that names itself is repeated when its whole value appears in
/// the claim. A labelled one is repeated when both its label and its
/// value do — a bare number can turn up in a summary for its own
/// reasons, and it is the pair that makes it the same fact.
fn repeats(summary: &str, fact: &Fact) -> bool {
    let value = fact.value.trim();
    if value.is_empty() {
        return false;
    }
    match &fact.label {
        None => summary.contains(value),
        Some(label) => summary.contains(label.trim()) && summary.contains(value),
    }
}

impl std::ops::Deref for Escalation {
    type Target = GateWaitingPayload;

    fn deref(&self) -> &GateWaitingPayload {
        &self.0
    }
}
