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
use yunta_core::events::{GateResolvedPayload, GateWaitingPayload};

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
