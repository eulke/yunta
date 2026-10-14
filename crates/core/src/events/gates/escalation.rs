//! An escalation on its way to a person: the claim, the record it is audited
//! against, and the menu of answers, with the reasons an answer is refused.

use serde::{Deserialize, Serialize};

use super::payloads::{DepartureMenu, FailureMenu, GateOption, GateWaitingPayload, Shown};
use crate::events::{Evidence, Fact};
use crate::ids::{NodeId, OptionId};
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

/// What an escalation says happened: a claim the engine states about a node,
/// or a summary whoever escalated wrote.
///
/// On the log an engine claim is an object tagged by `claim`, and a plain
/// string reads back as a summary the escalating party wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(from = "ClaimWire", into = "ClaimWire")]
#[schemars(with = "ClaimWire")]
pub enum Claim {
    /// A gate's question, which a person decides.
    NeedsDecision { node: NodeId },
    /// A node that failed, whose menu of answers a person chooses from.
    NodeFailed { node: NodeId },
    /// A gate whose pull request waits for an external review, and the
    /// assignee the gate names, when it names one.
    AwaitsReview {
        node: NodeId,
        assignee: Option<String>,
    },
    /// An external gate whose pull request cannot be published, because no
    /// forge is reachable from this machine, and the assignee the gate names.
    ForgeUnreachable { node: NodeId, assignee: String },
    /// A published external gate whose review cannot be checked again, because
    /// no forge is reachable from this machine, and the reference its
    /// publication left on the log.
    RecheckUnreachable { node: NodeId, external_ref: String },
    /// A summary the escalating party wrote.
    Written(String),
}

/// The persisted form of a [`Claim`]: a summary as its plain text, or an
/// engine claim as an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
enum ClaimWire {
    Written(String),
    Stated(StatedClaim),
}

/// An engine claim as the log holds it, tagged by `claim`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "claim", rename_all = "snake_case")]
enum StatedClaim {
    NeedsDecision {
        node: NodeId,
    },
    NodeFailed {
        node: NodeId,
    },
    AwaitsReview {
        node: NodeId,
        #[serde(skip_serializing_if = "Option::is_none")]
        assignee: Option<String>,
    },
    ForgeUnreachable {
        node: NodeId,
        assignee: String,
    },
    RecheckUnreachable {
        node: NodeId,
        external_ref: String,
    },
}

impl From<Claim> for ClaimWire {
    fn from(claim: Claim) -> Self {
        match claim {
            Claim::NeedsDecision { node } => ClaimWire::Stated(StatedClaim::NeedsDecision { node }),
            Claim::NodeFailed { node } => ClaimWire::Stated(StatedClaim::NodeFailed { node }),
            Claim::AwaitsReview { node, assignee } => {
                ClaimWire::Stated(StatedClaim::AwaitsReview { node, assignee })
            }
            Claim::ForgeUnreachable { node, assignee } => {
                ClaimWire::Stated(StatedClaim::ForgeUnreachable { node, assignee })
            }
            Claim::RecheckUnreachable { node, external_ref } => {
                ClaimWire::Stated(StatedClaim::RecheckUnreachable { node, external_ref })
            }
            Claim::Written(text) => ClaimWire::Written(text),
        }
    }
}

impl From<ClaimWire> for Claim {
    fn from(wire: ClaimWire) -> Self {
        match wire {
            ClaimWire::Written(text) => Claim::Written(text),
            ClaimWire::Stated(StatedClaim::NeedsDecision { node }) => Claim::NeedsDecision { node },
            ClaimWire::Stated(StatedClaim::NodeFailed { node }) => Claim::NodeFailed { node },
            ClaimWire::Stated(StatedClaim::AwaitsReview { node, assignee }) => {
                Claim::AwaitsReview { node, assignee }
            }
            ClaimWire::Stated(StatedClaim::ForgeUnreachable { node, assignee }) => {
                Claim::ForgeUnreachable { node, assignee }
            }
            ClaimWire::Stated(StatedClaim::RecheckUnreachable { node, external_ref }) => {
                Claim::RecheckUnreachable { node, external_ref }
            }
        }
    }
}

impl From<String> for Claim {
    fn from(text: String) -> Self {
        Claim::Written(text)
    }
}

impl From<&str> for Claim {
    fn from(text: &str) -> Self {
        Claim::Written(text.to_string())
    }
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
    /// One escalation, refused if its written summary repeats its record.
    pub fn new(
        summary: impl Into<Claim>,
        evidence: Evidence,
        options: NonEmpty<GateOption>,
    ) -> Result<Self, EscalationError> {
        let summary = summary.into();
        refuse_repeat(&summary, &evidence)?;
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
        summary: impl Into<Claim>,
        evidence: Evidence,
        external_ref: impl Into<String>,
    ) -> Result<Self, EscalationError> {
        let summary = summary.into();
        refuse_repeat(&summary, &evidence)?;
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

/// Refuses a written summary that repeats a fact the record holds. An engine
/// claim names its node and states no fact in words, so it repeats none.
fn refuse_repeat(summary: &Claim, evidence: &Evidence) -> Result<(), EscalationError> {
    let Claim::Written(text) = summary else {
        return Ok(());
    };
    match evidence.facts().iter().find(|fact| repeats(text, fact)) {
        Some(fact) => Err(EscalationError::SummaryRepeatsEvidence { fact: fact.clone() }),
        None => Ok(()),
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
