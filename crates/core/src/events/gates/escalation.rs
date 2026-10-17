//! An escalation on its way to a person: the claim, the menu of answers it is
//! about, and the reasons an answer is refused.

use serde::{Deserialize, Serialize};

use super::menu::{Menu, PublishedGate};
use super::offer::{Asks, GateOption};
use super::payloads::{GateWaitingPayload, Shown};
use super::withheld::Withheld;
use crate::events::Evidence;
use crate::ids::{NodeId, OptionId, RunId, TaskId};
use crate::NonEmpty;

/// Why an answer does not count as a decision on an escalation. Each
/// variant's display is its code; the words a reader sees are the caller's.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// The option is not on the menu, and `offered` lists the options it holds.
    #[error("off_menu")]
    OffMenu { chosen: OptionId, offered: String },
    /// The option asks for words, and the answer has none.
    #[error("unsaid")]
    Unsaid { chosen: OptionId, asks: Asks },
    /// The gate withholds the option, and `withheld` says why.
    #[error("withheld")]
    Withheld { withheld: Withheld },
}

/// What an escalation says happened: a claim the engine states about a node
/// or the run, or a summary whoever escalated wrote.
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
    /// The run spent its token cap.
    TokensSpent { run: RunId },
    /// A loop wants another iteration, and its cap is spent.
    LoopCapSpent { node: NodeId },
    /// A loop's task asks for scope beyond its own; `reason` is its session's
    /// words, absent when a red criterion of the task located the refused writes.
    ScopeAsked {
        task: TaskId,
        reason: Option<String>,
    },
    /// A task's session departs from the plan.
    Departs { task: TaskId },
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
    TokensSpent {
        run: RunId,
    },
    LoopCapSpent {
        node: NodeId,
    },
    ScopeAsked {
        task: TaskId,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Departs {
        task: TaskId,
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
            Claim::TokensSpent { run } => ClaimWire::Stated(StatedClaim::TokensSpent { run }),
            Claim::LoopCapSpent { node } => ClaimWire::Stated(StatedClaim::LoopCapSpent { node }),
            Claim::ScopeAsked { task, reason } => {
                ClaimWire::Stated(StatedClaim::ScopeAsked { task, reason })
            }
            Claim::Departs { task } => ClaimWire::Stated(StatedClaim::Departs { task }),
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
            ClaimWire::Stated(StatedClaim::TokensSpent { run }) => Claim::TokensSpent { run },
            ClaimWire::Stated(StatedClaim::LoopCapSpent { node }) => Claim::LoopCapSpent { node },
            ClaimWire::Stated(StatedClaim::ScopeAsked { task, reason }) => {
                Claim::ScopeAsked { task, reason }
            }
            ClaimWire::Stated(StatedClaim::Departs { task }) => Claim::Departs { task },
        }
    }
}

/// An escalation on its way to a person: the claim, the menu of answers it
/// is about, and the options it offers.
///
/// The one way a `gate_waiting` payload comes to exist: its claim and its
/// menu are given here, once, and the options cannot be empty by type, since
/// an escalation with no options refuses every answer it is given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(from = "GateWaitingPayload", into = "GateWaitingPayload")]
#[schemars(with = "GateWaitingPayload")]
pub struct Escalation(pub(super) GateWaitingPayload);

impl Escalation {
    /// One escalation: what it claims, what it is about, and the menu of answers.
    pub fn new(summary: Claim, menu: Menu, options: NonEmpty<GateOption>) -> Self {
        Escalation(GateWaitingPayload {
            summary,
            evidence: Evidence::default(),
            options: options.into_vec(),
            external_ref: None,
            shows: Vec::new(),
            withheld: Vec::new(),
            menu: Some(menu),
        })
    }

    /// An escalation whose answer comes from a forge: the pull request is the
    /// menu, and `external_ref` is the handle a later poll needs to find it
    /// again from a process that knows nothing but the log.
    ///
    /// No options, and that is the shape rather than an omission: no answer
    /// given here would count, because the decision is made and recorded on
    /// the forge. `offers` refusing every option is the truth about this gate,
    /// not the bug it would be on a local one.
    pub fn published_to(
        summary: Claim,
        published: PublishedGate,
        external_ref: impl Into<String>,
    ) -> Self {
        Escalation(GateWaitingPayload {
            summary,
            evidence: Evidence::default(),
            options: Vec::new(),
            external_ref: Some(external_ref.into()),
            shows: Vec::new(),
            withheld: Vec::new(),
            menu: Some(Menu::Published(published)),
        })
    }

    /// The same escalation, putting `shows` in front of the person.
    pub fn showing(mut self, shows: Vec<Shown>) -> Self {
        self.0.shows = shows;
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

impl std::ops::Deref for Escalation {
    type Target = GateWaitingPayload;

    fn deref(&self) -> &GateWaitingPayload {
        &self.0
    }
}
