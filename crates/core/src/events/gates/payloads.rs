//! A decision a person makes: an escalation waiting on one, the
//! resolution that answered it, and the questions a node asked.

use serde::{Deserialize, Serialize};

use super::escalation::Refusal;
use crate::events::session::payloads::TokenUsage;
use crate::events::tasks::DeviationDeclaredPayload;
use crate::events::{Evidence, Failure, Remedy, Stuck};
use crate::hash::{CommitSha, ContentHash};
use crate::ids::{ModeName, NodeId, OptionId, QuestionId, Responder};

/// One choice in a gate's escalation: `id` is what
/// `GateResolvedPayload.chosen_option` names back. `label` and `tradeoff`
/// are the words a person reads beside it, when the option stores them.
/// An option that stores neither has its words written when it is shown,
/// from the data its escalation carries, so neither is required here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GateOption {
    pub id: OptionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tradeoff: Option<String>,
    /// What choosing this option has to say along with it, when an
    /// answer without words would change nothing — a correction sent
    /// back to the session that made the work is the words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asks: Option<String>,
}

/// An artifact an escalation puts in front of the person deciding: the
/// version they saw, by its hash, so a decision on it is a decision on
/// exactly those bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Shown {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<crate::ids::NodeId>,
    pub artifact: crate::events::ArtifactId,
    pub content_hash: ContentHash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Tty,
    Mcp,
    /// Nobody answered: every question was optional, and the engine closed
    /// the round on what each one assumes.
    Assumed,
}

impl Channel {
    /// The word this channel is named by, as the log spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Tty => "tty",
            Channel::Mcp => "mcp",
            Channel::Assumed => "assumed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GateWaitingPayload {
    /// The claim: what happened, in the words of whoever escalated.
    /// It ends where the record begins — a summary that quotes what
    /// `evidence` holds leaves every surface printing it twice.
    pub(super) summary: String,
    /// The record `summary` is audited against, attached by the engine
    /// straight from the log.
    pub(super) evidence: Evidence,
    pub(super) options: Vec<GateOption>,
    /// The forge's own handle for this gate, such as a pull request's
    /// URL; `None` for an internal escalation, such as exhausted
    /// re-routes. It carries the forge's `PullRequestRef` through the
    /// log, so a later `poll` from another process knows what to poll
    /// without publishing again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) external_ref: Option<String>,
    /// The artifacts the decision is about, as the person was shown
    /// them. Empty for an escalation about something that happened.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) shows: Vec<Shown>,
    /// The options the gate does not offer, each with why: what stands
    /// against them is part of the decision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) withheld: Vec<super::Withheld>,
    /// Why the loop waits on a scope request a person answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) scope: Option<super::ScopeWait>,
    /// The failure a node's escalation is about, with the menu that
    /// failure offers. Absent on every other escalation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<Box<FailureMenu>>,
    /// The departure from the plan a task's session declared, as the
    /// question about it is written. Absent on every other escalation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub departure: Option<Box<DepartureMenu>>,
}

impl GateWaitingPayload {
    /// The claim: what happened, in the words of whoever escalated.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// The record the claim is audited against.
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }

    /// The answers on offer.
    pub fn options(&self) -> &[GateOption] {
        &self.options
    }

    /// The payload with each option's label and tradeoff set from `words`,
    /// the words a person reads beside it. A surface that shows the menu
    /// applies this to its own copy: an option that stores no words is then
    /// shown with the ones its escalation gives it.
    pub fn worded(
        mut self,
        words: impl Fn(&Self, &GateOption) -> (String, Option<String>),
    ) -> Self {
        let filled: Vec<(String, Option<String>)> = self
            .options
            .iter()
            .map(|option| words(&self, option))
            .collect();
        for (option, (label, tradeoff)) in self.options.iter_mut().zip(filled) {
            option.label = Some(label);
            option.tradeoff = tradeoff;
        }
        self
    }

    /// The forge's own handle for this gate, when it has one.
    pub fn external_ref(&self) -> Option<&str> {
        self.external_ref.as_deref()
    }

    /// The artifacts the decision is about, as the person saw them.
    pub fn shows(&self) -> &[Shown] {
        &self.shows
    }

    /// Whether `option` is on this escalation's menu.
    pub fn offers(&self, option: &OptionId) -> bool {
        self.options.iter().any(|o| o.id == *option)
    }

    /// Whether `choice` counts as a decision on this escalation: an
    /// option on its menu, with words when the option asks for them.
    /// The one test every answer passes, whichever surface gave it.
    pub fn accepts(&self, choice: &HumanChoice) -> Result<(), Refusal> {
        if let Some(withheld) = self.refused_as_withheld(&choice.option) {
            return Err(withheld);
        }
        let Some(option) = self.options.iter().find(|o| o.id == choice.option) else {
            return Err(Refusal::OffMenu {
                chosen: choice.option.clone(),
                offered: self.menu(),
            });
        };
        let said = choice
            .free_text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty());
        match &option.asks {
            Some(asks) if !said => Err(Refusal::Unsaid {
                chosen: choice.option.clone(),
                asks: asks.clone(),
            }),
            _ => Ok(()),
        }
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

/// A person at the engine's terminal is being asked something: a gate's
/// choice, an escalation, a round of questions. The answer lands as the
/// event that resolves what was asked; this one says when the asking
/// began, which only the engine that put the question knows. Audit: no
/// ledger moves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AskingOpenedPayload {
    /// The task whose work is waiting on the answer, when a task's
    /// session asked; absent when the node asked for itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<crate::ids::TaskId>,
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

/// The failure a node's escalation is about, and the menu it offers: the
/// attempt the choices start, the failure as data, the remedies that can
/// change its outcome and, when none can, why not. Every word of the menu
/// is written from these fields when it is shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FailureMenu {
    /// The attempt the menu's choices start, numbered as the log numbers
    /// attempts.
    pub attempt: u32,
    pub failure: Failure,
    pub remedies: Vec<Remedy>,
    /// Why no remedy is offered, when none is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stuck: Option<Stuck>,
    /// The re-route this failure exhausted, when it closes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reroute: Option<ExhaustedReroute>,
}

/// A re-route the failure exhausted: the node it would send the run to,
/// the cap it reached, and the mode a promotion would take the run to,
/// when the workflow names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExhaustedReroute {
    pub goto: NodeId,
    pub max_reroutes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_mode: Option<ModeName>,
}

/// The departure from the plan a task's session declared, as its question
/// is written: the declarations it asks about, the plan's criteria that
/// accepting stops holding the task to, the node that writes the task's
/// tests again, and whether accepting is offered at all. Every word of the
/// question is written from these fields when it is shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DepartureMenu {
    pub deviations: Vec<DeviationDeclaredPayload>,
    /// The commands of the plan's criteria accepting stops holding the
    /// task to. Absent when accepting waives none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waived: Vec<String>,
    /// The node that writes the task's tests again, when accepting does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewritten_by: Option<NodeId>,
    /// Whether accepting is offered. False when a test a departure departs
    /// from came with the run and no node of the run writes it again.
    pub accept_offered: bool,
}
