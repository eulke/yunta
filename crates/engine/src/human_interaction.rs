//! `HumanInteraction` — the trait a gate's escalation
//! resolves through. The same object is rendered on every surface
//! (console, the MCP `resolve_gate` tool) — never a per-surface
//! reinterpretation. One trait, one method, the log's own types in
//! and out (`GateWaitingPayload` is the escalation as persisted;
//! `HumanChoice` is the content of the `gate_resolved` the log records)
//! is what makes "no duplicated logic" true by construction: there is
//! nowhere for a second interpretation of a gate to live. A surface
//! only ever chooses from the menu it was shown; the shapes a forge
//! produces (an approval, a review) are not a surface's to return.
//!
//! The console implementation lives in `yunta-cli` (the engine has no
//! concrete UI and never writes to the console itself); the MCP one is a
//! `resolve_gate` tool, not built here.

use async_trait::async_trait;
use yunta_core::events::{Channel, GateWaitingPayload, HumanChoice};
use yunta_core::{Answer, QuestionsFile, Responder};

/// What a decision is asked with, beside the escalation the log records:
/// the documents the escalation shows, read from the run for the person
/// to see. The escalation names them by hash; this is their content.
pub struct Asking<'a> {
    pub shown: &'a [ShownDocument],
    /// The run the decision is in, which a surface names a document by.
    pub run: &'a yunta_core::RunId,
}

pub use yunta_core::shown::{ShownContent, ShownDocument};

/// One surface's reply to a `kind: questions` artifact:
/// the answers plus which channel produced them and who answered — the
/// implementation knows its own channel (console = `Tty`, the MCP tool
/// = `Mcp`), the engine only records it into `questions_answered`.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestionsReply {
    pub answers: Vec<Answer>,
    pub channel: Channel,
    pub responder: Option<Responder>,
}

/// Resolves one gate's escalation, or reports that this surface can't
/// interact right now. `None` is not a failure — a headless run, a
/// piped/non-TTY invocation, or `yunta test`'s mock-driven runs all
/// legitimately have nothing to ask a human, and the caller degrades to
/// pausing rather than hanging without a TTY, the same way `kind:
/// questions` already applies it.
/// `default_on_timeout: none` is enforced by
/// this trait having no timeout parameter at all — a surface that
/// wanted to time out would have to invent its own auto-decision, which
/// the engine never allows.
#[async_trait]
pub trait HumanInteraction: Send + Sync {
    /// One option off `escalation`'s menu, with who chose it. The engine
    /// checks the pick against the menu before recording it; a surface
    /// that answers off the menu is a bug, not a decision.
    async fn resolve(&self, escalation: &GateWaitingPayload) -> Option<HumanChoice>;

    /// [`resolve`](Self::resolve), with what the asking knows: the
    /// documents the escalation shows. They are the asking's context, not
    /// the escalation's — the escalation is what the log records and a
    /// parked run rebuilds. A surface with nowhere to show them answers
    /// as `resolve` does.
    async fn resolve_in(
        &self,
        escalation: &GateWaitingPayload,
        _asking: &Asking<'_>,
    ) -> Option<HumanChoice> {
        self.resolve(escalation).await
    }

    /// Puts a `kind: questions` artifact to the human,
    /// question by question. `None` = this surface can't ask (same
    /// convention as `resolve`), and the run parks with its questions
    /// registered, to be answered on a later invocation or through
    /// another surface. Deliberately a separate method from `resolve`:
    /// questions and gate escalations are two distinct shapes, and
    /// flattening them into one payload would breed the
    /// ambiguous-object vice. A default implementation returns `None`
    /// so surfaces that only handle gates stay valid unchanged.
    ///
    /// How the questions are presented is the surface's own call: a
    /// node that declares them has said everything it has to say. `node`
    /// is the one asking, which a surface names so a person knows whose
    /// questions these are.
    async fn ask(
        &self,
        node: &yunta_core::NodeId,
        questions: &QuestionsFile,
    ) -> Option<QuestionsReply> {
        let _ = (node, questions);
        None
    }
}

/// Always reports "can't interact" — the default for any run not
/// explicitly wired to a live surface: engine tests, `yunta test`'s
/// mock-driven runs, headless CI. Never hangs, never guesses; a run
/// that hits a gate under this implementation simply pauses, exactly as
/// a gate does whenever nothing is watching for it.
pub struct NoInteraction;

#[async_trait]
impl HumanInteraction for NoInteraction {
    async fn resolve(&self, _escalation: &GateWaitingPayload) -> Option<HumanChoice> {
        None
    }
}
