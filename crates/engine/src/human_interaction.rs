//! `HumanInteraction` (T7.2, §5.3) — the trait a gate's escalation
//! resolves through. §5.3 is explicit that the object is normative and
//! the surface is not: "el mismo objeto se renderiza en toda superficie
//! (consola, tool MCP `resolve_gate`)". One trait, one method, one
//! object (`yunta_core::events::{GateWaitingPayload, GateResolvedPayload}`
//! — the same types the event log persists, not a parallel runtime
//! shape) is what makes "sin lógica duplicada" true by construction:
//! there is nowhere for a second interpretation of a gate to live.
//!
//! The console implementation lives in `yunta-cli` (A1: engine has no
//! concrete UI, CLAUDE.md's own "nunca `println!` fuera del CLI"); the
//! MCP one is M8's `resolve_gate` tool, not built here.

use async_trait::async_trait;
use yunta_core::events::{Channel, GateResolvedPayload, GateWaitingPayload};
use yunta_core::{Answer, QuestionsFile};

/// One surface's reply to a `kind: questions` artifact (DI-02, §4.1):
/// the answers plus which channel produced them and who answered — the
/// implementation knows its own channel (console = `Tty`, M8's MCP tool
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
/// pausing rather than hanging (§4.1's own "sin TTY... nunca cuelga",
/// applied here the same way `kind: questions` already applies it).
/// `default_on_timeout: none` from §5.3's own example is enforced by
/// this trait having no timeout parameter at all — a surface that
/// wanted to time out would have to invent its own auto-decision, which
/// is exactly what the Contrato forbids.
#[async_trait]
pub trait HumanInteraction: Send + Sync {
    async fn resolve(&self, escalation: &GateWaitingPayload) -> Option<GateResolvedPayload>;

    /// §4.1/D86 (DI-02): puts a `kind: questions` artifact to the human,
    /// question by question. `None` = this surface can't ask (same
    /// convention as `resolve`), and the run degrades to waiting exactly
    /// as it did before any surface existed. Deliberately a separate
    /// method from `resolve`: questions (§4.1) and gate escalations
    /// (§5.3) are two distinct normative shapes, and flattening them
    /// into one payload would breed the ambiguous-object vice. A default
    /// implementation returns `None` so surfaces that only handle gates
    /// (and every existing implementor) stay valid unchanged.
    /// `interactive` is the node's own `interactive:` flag (§4.1,
    /// DI-13) — a presentation datum: a surface that can hold a live
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
/// it did before T7.2 existed.
pub struct NoInteraction;

#[async_trait]
impl HumanInteraction for NoInteraction {
    async fn resolve(&self, _escalation: &GateWaitingPayload) -> Option<GateResolvedPayload> {
        None
    }
}
