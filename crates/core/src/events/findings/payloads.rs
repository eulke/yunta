//! What a session reported about work that is not its own to fix: posted,
//! updated, withdrawn, or refused by the engine.

use serde::{Deserialize, Serialize};

use super::cause::{FindingCause, FindingSays};
use crate::events::scope::payloads::ProposedCriterion;
use crate::findings::Location;
use crate::ids::{FindingId, NodeId};

/// How much a finding matters, from `blocking` down to `note`.
// Confirmed against the `kind: findings` schema, not inferred. Declared
// the way the ladder reads, the most severe first, which is what `Ord`
// follows.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Blocking,
    Major,
    Minor,
    Note,
}

impl FindingSeverity {
    /// The ladder, as a document writes it. Tied to what serde derives
    /// by a test, so a diagnostic listing the ladder cannot list a
    /// different one from the parser accepting it.
    pub const NAMES: [&'static str; 4] = ["blocking", "major", "minor", "note"];

    /// This severity's own rung of that ladder — the one place it is
    /// named, so a document, a diagnostic and a surface spell it alike.
    pub fn as_str(self) -> &'static str {
        match self {
            FindingSeverity::Blocking => "blocking",
            FindingSeverity::Major => "major",
            FindingSeverity::Minor => "minor",
            FindingSeverity::Note => "note",
        }
    }
}

/// A finding as the run holds it. Its log form keeps a session's title and
/// detail in the order a session always wrote them, or the engine's cause
/// in their place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "FindingWire", into = "FindingWire")]
#[schemars(with = "FindingWire")]
pub struct Finding {
    /// Its id, unique among the findings its author posted.
    pub id: FindingId,
    /// How much it matters: what a `findings_gate` holds the run to.
    pub severity: FindingSeverity,
    /// What it says: a session's own words, or what the engine found.
    pub says: FindingSays,
    /// Where it is. Two findings at one place that make one complaint are
    /// counted once.
    pub location: Location,
    /// A command that would prove it fixed, when its author proposes one.
    pub proposed_criterion: Option<ProposedCriterion>,
}

/// A finding as the log holds it: a session's title and detail in today's
/// order, or the engine's cause, never both.
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct FindingWire {
    id: FindingId,
    severity: FindingSeverity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    location: Location,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cause: Option<FindingCause>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proposed_criterion: Option<ProposedCriterion>,
}

/// A log line that says neither a session's title and detail nor the
/// engine's cause, or says both.
#[derive(Debug, thiserror::Error)]
#[error("a finding says its title and detail, or its cause")]
pub struct UnsaidFinding;

impl From<Finding> for FindingWire {
    fn from(finding: Finding) -> Self {
        let (title, detail, cause) = match finding.says {
            FindingSays::Written { title, detail } => (Some(title), Some(detail), None),
            FindingSays::Found { cause } => (None, None, Some(cause)),
        };
        FindingWire {
            id: finding.id,
            severity: finding.severity,
            title,
            location: finding.location,
            detail,
            cause,
            proposed_criterion: finding.proposed_criterion,
        }
    }
}

impl TryFrom<FindingWire> for Finding {
    type Error = UnsaidFinding;

    fn try_from(wire: FindingWire) -> Result<Self, UnsaidFinding> {
        let says = match (wire.title, wire.detail, wire.cause) {
            (Some(title), Some(detail), None) => FindingSays::Written { title, detail },
            (None, None, Some(cause)) => FindingSays::Found { cause },
            _ => return Err(UnsaidFinding),
        };
        Ok(Finding {
            id: wire.id,
            severity: wire.severity,
            says,
            location: wire.location,
            proposed_criterion: wire.proposed_criterion,
        })
    }
}

/// The finding's author is the envelope's own `node_id` — not repeated
/// here. A finding with no node is the engine's own, about the run: each
/// degradation it suffers is posted as one, never a warning that leaves
/// the log silent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingPostedPayload {
    pub finding: Finding,
}

/// The node's finding, in its new state — a whole replacement, never a
/// merge: a field absent from an update is absent from the finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingUpdatedPayload {
    pub finding: Finding,
}

/// The node's finding `id` no longer stands, and why — in the words of
/// whoever took it back. A withdrawal is final: the id is never posted,
/// updated or withdrawn again, and a finding that comes back is a new id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingWithdrawnPayload {
    pub id: FindingId,
    pub reason: String,
}

/// What a node answered about a finding another node reported: its work
/// fixed it, or it declines to, and why. The answering node is the
/// envelope's own `node_id`; `node` and `id` name the finding answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingAnsweredPayload {
    pub node: NodeId,
    pub id: FindingId,
    pub answer: FindingAnswer,
    pub why: String,
}

/// What a node's work proved of a finding it answered fixed: the
/// criterion the finding proposes, run on the tree the node left. Passing
/// settles the finding; failing is kept, so a person reads that the
/// evidence did not bear the answer out. The proving node is the
/// envelope's own `node_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingProvedPayload {
    pub node: NodeId,
    pub id: FindingId,
    pub result: crate::events::CriterionResult,
}

/// A person went on past a gate that showed them a finding, which
/// settles it: it stands, and no longer counts against the run. The gate
/// is the envelope's own `node_id`; `caused_by` is the decision that
/// settled it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingSettledPayload {
    /// The node that reported the finding; absent for one the engine
    /// reported about the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    pub id: FindingId,
    pub caused_by: crate::ids::Seq,
}

/// What an answer says of a finding: fixed by the answering node's work,
/// or declined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingAnswer {
    Fixed,
    Declined,
}

impl FindingAnswer {
    /// The word this answer is named by, as the log spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            FindingAnswer::Fixed => "fixed",
            FindingAnswer::Declined => "declined",
        }
    }
}

/// Which of the three a refused call was making.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingOperation {
    Post,
    Update,
    Withdraw,
}

impl FindingOperation {
    /// The word this call is named by, as the log spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            FindingOperation::Post => "post",
            FindingOperation::Update => "update",
            FindingOperation::Withdraw => "withdraw",
        }
    }
}

/// A finding a session offered and the engine did not take, with every
/// problem named. The accepted cases are the three events above; this is
/// what a session was told instead, kept so the rate a run gets findings
/// wrong is a fact about the run rather than something only the session
/// saw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindingRefusedPayload {
    pub operation: FindingOperation,
    /// The id the call named, when it named one that parses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<FindingId>,
    pub report: crate::diagnostic::Report,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written() -> Finding {
        Finding {
            id: "f-1".into(),
            severity: FindingSeverity::Major,
            says: FindingSays::Written {
                title: "t".to_string(),
                detail: "d".to_string(),
            },
            location: "src/a.rs:1".into(),
            proposed_criterion: None,
        }
    }

    /// A session's finding writes the keys it always wrote, in the order it
    /// always wrote them.
    #[test]
    fn a_session_finding_writes_the_keys_it_always_wrote() {
        assert_eq!(
            serde_json::to_string(&written()).expect("a finding serializes"),
            r#"{"id":"f-1","severity":"major","title":"t","location":"src/a.rs:1","detail":"d"}"#,
        );
    }

    /// An engine's finding writes its cause where a session's words go, and
    /// reads back as it was written.
    #[test]
    fn an_engine_finding_writes_its_cause_and_reads_back() {
        let found = Finding {
            says: FindingSays::Found {
                cause: FindingCause::CleanupSkipped,
            },
            ..written()
        };
        let text = serde_json::to_string(&found).expect("a finding serializes");
        assert_eq!(
            text,
            r#"{"id":"f-1","severity":"major","location":"src/a.rs:1","cause":"cleanup_skipped"}"#,
        );
        assert_eq!(
            serde_json::from_str::<Finding>(&text).expect("it reads back"),
            found
        );
    }

    /// A line that says both its words and its cause, or neither, is not a
    /// finding.
    #[test]
    fn a_finding_says_its_words_or_its_cause_and_never_both() {
        let both = r#"{"id":"f-1","severity":"major","title":"t","location":"src/a.rs:1","detail":"d","cause":"cleanup_skipped"}"#;
        let neither = r#"{"id":"f-1","severity":"major","location":"src/a.rs:1"}"#;
        assert!(serde_json::from_str::<Finding>(both).is_err());
        assert!(serde_json::from_str::<Finding>(neither).is_err());
    }
}
