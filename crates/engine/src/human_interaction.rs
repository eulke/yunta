//! `HumanInteraction` — the trait a gate's escalation
//! resolves through. The same object is rendered on every surface
//! (console, the MCP `resolve_gate` tool) — never a per-surface
//! reinterpretation. One trait, one method, one
//! object (`yunta_core::events::{GateWaitingPayload, GateResolvedPayload}`
//! — the same types the event log persists, not a parallel runtime
//! shape) is what makes "no duplicated logic" true by construction:
//! there is nowhere for a second interpretation of a gate to live.
//!
//! The console implementation lives in `yunta-cli` (the engine has no
//! concrete UI and never writes to the console itself); the MCP one is a
//! `resolve_gate` tool, not built here.

use async_trait::async_trait;
use yunta_core::events::{Channel, GateResolvedPayload, GateWaitingPayload};
use yunta_core::{Answer, QuestionsFile};

/// One surface's reply to a `kind: questions` artifact:
/// the answers plus which channel produced them and who answered — the
/// implementation knows its own channel (console = `Tty`, the MCP tool
/// = `Mcp`), the engine only records it into `questions_answered`.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestionsReply {
    pub answers: Vec<Answer>,
    pub channel: Channel,
    pub responder: Option<String>,
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
    async fn resolve(&self, escalation: &GateWaitingPayload) -> Option<GateResolvedPayload>;

    /// Puts a `kind: questions` artifact to the human,
    /// question by question. `None` = this surface can't ask (same
    /// convention as `resolve`), and the run degrades to waiting exactly
    /// as it did before any surface existed. Deliberately a separate
    /// method from `resolve`: questions and gate escalations
    /// are two distinct shapes, and flattening them
    /// into one payload would breed the ambiguous-object vice. A default
    /// implementation returns `None` so surfaces that only handle gates
    /// (and every existing implementor) stay valid unchanged.
    /// `interactive` is the node's own `interactive:` flag — a
    /// presentation datum: a surface that can hold a live
    /// conversation should when it's `true`; one that can't ignores it,
    /// and nothing else changes.
    async fn ask(&self, questions: &QuestionsFile, interactive: bool) -> Option<QuestionsReply> {
        let _ = (questions, interactive);
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
    async fn resolve(&self, _escalation: &GateWaitingPayload) -> Option<GateResolvedPayload> {
        None
    }
}
