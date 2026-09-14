//! A decision a person makes: an escalation waiting on one, the
//! resolution that answered it, and the questions a node asked.

use serde::{Deserialize, Serialize};

use crate::events::session::payloads::TokenUsage;
use crate::events::Evidence;
use crate::hash::{CommitSha, ContentHash};
use crate::ids::{OptionId, QuestionId, Responder};

/// One choice in a gate's escalation: `id` is what
/// `GateResolvedPayload.chosen_option` names back, `label` is the
/// human-facing text, `tradeoff` is mandatory — any option that expands
/// scope of work must declare what it trades off, and no variant of
/// this type can omit it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GateOption {
    pub id: OptionId,
    pub label: String,
    pub tradeoff: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Tty,
    Mcp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GateWaitingPayload {
    /// The claim: what happened, in the words of whoever escalated.
    /// It ends where the record begins — a summary that quotes what
    /// `evidence` holds leaves every surface printing it twice.
    pub summary: String,
    /// The record `summary` is audited against, attached by the engine
    /// straight from the log.
    pub evidence: Evidence,
    pub options: Vec<GateOption>,
    /// The forge's own handle for this gate — a PR URL,
    /// today — `None` for the internal escalation case (exhausted
    /// re-routes) this payload already covered before external
    /// gates existed. Round-trips the forge's `PublishedGate` through
    /// the log so a later `poll` (from a completely different process
    /// waking up to check on the gate) knows what to poll without
    /// re-publishing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
}

impl GateWaitingPayload {
    /// Whether `option` is on this escalation's menu: the one test an
    /// answer passes before it counts as a decision on it.
    pub fn offers(&self, option: &OptionId) -> bool {
        self.options.iter().any(|o| o.id == *option)
    }

    /// The escalation on one line — its claim, then the facts behind
    /// it — for a surface with room for exactly one: the reason a
    /// `run_paused` records, and the row a listing gives a run.
    ///
    /// A surface with room for two parts heads each separately; a line
    /// has room for neither heading. Composing them here is what keeps
    /// the page and the line from disagreeing about what an escalation
    /// says.
    pub fn sentence(&self) -> String {
        crate::text::aside(&self.summary, &self.evidence.one_line())
    }

    /// The menu's option ids as one comma-separated line, for a message
    /// that names what was offered.
    pub fn menu(&self) -> String {
        self.options
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// How a gate's escalation was settled. On the wire this is one flat
/// object of four optional fields, and the fields present spell the
/// shape: an option with its responder is a human's `Chosen`; a
/// responder with a commit id is the forge's `Approved`; a responder
/// alone is `ChangesRequested`; nothing at all is `Closed`. Reading
/// decides the shape once, here, so every reader matches on it instead
/// of inferring it from which field is set. A combination no shape
/// names reads as `Unrecognized` and writes back verbatim: a newer
/// writer may mean something by it, and the export loses nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(from = "GateResolvedWire", into = "GateResolvedWire")]
pub enum GateResolvedPayload {
    /// A human picked one of the escalation's options: an internal
    /// gate, exhausted re-routes, a token budget, a scope expansion, or
    /// an external gate degraded to the console.
    Chosen(HumanChoice),
    /// The forge reports an approving review, or a merge, covering
    /// `sha`. A merge is an approval whose evidence is the merge commit.
    Approved { by: Responder, sha: CommitSha },
    /// The forge reports a changes-requested review by `by`.
    ChangesRequested { by: Responder },
    /// The pull request was closed without merging.
    Closed,
    /// A combination of fields no shape above names, kept as read. Only
    /// reading produces it; nothing in this workspace writes one.
    Unrecognized(UnrecognizedResolution),
}

/// One option picked from an escalation's menu, and who picked it: the
/// content of [`GateResolvedPayload::Chosen`], and the only shape a
/// human-facing surface produces. A console or an MCP tool chooses; it
/// never reports an approval a forge did not give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanChoice {
    pub option: OptionId,
    pub by: Responder,
    pub free_text: Option<String>,
}

/// A `gate_resolved` whose fields spell no shape this binary names.
/// Opaque: it exists to be written back unchanged, never to be read
/// into a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnrecognizedResolution(GateResolvedWire);

/// The persisted object behind [`GateResolvedPayload`]: four optional
/// fields, the same for every shape. Serialization, deserialization and
/// the JSON Schema all go through it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
struct GateResolvedWire {
    /// The option a human chose from the menu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chosen_option: Option<OptionId>,
    /// Who decided: the human who chose, or the reviewer or merger the
    /// forge reports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resolved_by: Option<Responder>,
    /// Free-form context a human gave alongside the choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    free_text: Option<String>,
    /// The commit the forge's approval covers: what a later drift check
    /// compares against the pull request's current head to decide
    /// whether the approval still holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    approved_sha: Option<CommitSha>,
}

impl From<GateResolvedWire> for GateResolvedPayload {
    fn from(wire: GateResolvedWire) -> Self {
        match wire {
            GateResolvedWire {
                chosen_option: Some(option),
                resolved_by: Some(by),
                free_text,
                approved_sha: None,
            } => Self::Chosen(HumanChoice {
                option,
                by,
                free_text,
            }),
            GateResolvedWire {
                chosen_option: None,
                resolved_by: Some(by),
                free_text: None,
                approved_sha: Some(sha),
            } => Self::Approved { by, sha },
            GateResolvedWire {
                chosen_option: None,
                resolved_by: Some(by),
                free_text: None,
                approved_sha: None,
            } => Self::ChangesRequested { by },
            GateResolvedWire {
                chosen_option: None,
                resolved_by: None,
                free_text: None,
                approved_sha: None,
            } => Self::Closed,
            other => Self::Unrecognized(UnrecognizedResolution(other)),
        }
    }
}

impl From<GateResolvedPayload> for GateResolvedWire {
    fn from(payload: GateResolvedPayload) -> Self {
        match payload {
            GateResolvedPayload::Chosen(HumanChoice {
                option,
                by,
                free_text,
            }) => GateResolvedWire {
                chosen_option: Some(option),
                resolved_by: Some(by),
                free_text,
                approved_sha: None,
            },
            GateResolvedPayload::Approved { by, sha } => GateResolvedWire {
                resolved_by: Some(by),
                approved_sha: Some(sha),
                ..GateResolvedWire::default()
            },
            GateResolvedPayload::ChangesRequested { by } => GateResolvedWire {
                resolved_by: Some(by),
                ..GateResolvedWire::default()
            },
            GateResolvedPayload::Closed => GateResolvedWire::default(),
            GateResolvedPayload::Unrecognized(UnrecognizedResolution(wire)) => wire,
        }
    }
}

/// A node handed its questions over and closed on them: what it asked
/// from, which ids await an answer, and what the session that asked
/// spent.
///
/// The pair of [`QuestionsAnsweredPayload`]. Between the two the node
/// waits, and the `node_finished` its close deferred lands after the
/// answer — so a node that asked is never mistaken for one that failed,
/// and a log that holds a questions document is never mistaken for a
/// node that is waiting on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QuestionsAskedPayload {
    /// The questions document the node handed over: what the answers
    /// answer, and what the round re-reads before putting them to
    /// anyone.
    pub questions_hash: ContentHash,
    /// The ids awaiting an answer. Never empty: a node with nothing to
    /// ask finishes in the same close instead of waiting.
    pub questions: Vec<QuestionId>,
    /// What the session that asked spent. The attempt's accounting
    /// closes here, so the `node_finished` after the answer carries
    /// none and no surface counts the session twice while it waits.
    pub tokens_used: TokenUsage,
}

impl QuestionsAskedPayload {
    /// The fact, with the questions that make it one.
    ///
    /// `None` for an empty list: a node that asked nothing did not ask,
    /// and the caller finishes it instead of recording a wait nobody
    /// can end.
    pub fn new(
        questions_hash: ContentHash,
        questions: Vec<QuestionId>,
        tokens_used: TokenUsage,
    ) -> Option<Self> {
        (!questions.is_empty()).then_some(Self {
            questions_hash,
            questions,
            tokens_used,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QuestionsAnsweredPayload {
    pub answers_hash: ContentHash,
    pub channel: Channel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responder: Option<Responder>,
}
