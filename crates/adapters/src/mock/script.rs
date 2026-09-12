//! Playing one claimed script: the machine that turns a fixture's steps
//! into the event stream of a session.
//!
//! The machine is deliberately separate from claiming a script and
//! assembling the session around it. Once it starts it holds nothing of
//! the adapter — only the script it plays, the channel it plays into and
//! the two stops it answers to — so a played session cannot reach the
//! fixture other sessions are still claiming from, and a stop ends it
//! wherever it is.
//!
//! Every send is checked: a receiver that is gone means the engine
//! stopped reading this session, and a script that keeps playing into a
//! closed channel is work nobody asked for.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Notify};
use yunta_core::{ModelName, SessionId};

use super::run_tool::call_run_tool;
use super::{MockOutcome, MockStep, OnInterrupt, ToolExpectation};
use crate::session::{AgentError, AgentEvent, AgentOutcome};
use crate::RunToolsEndpoint;

/// One session's script, cut loose from the fixture it was claimed from.
pub(super) struct Script {
    pub(super) session_id: SessionId,
    pub(super) model: ModelName,
    /// The edits this session's effects would have made and the request's
    /// constraints blocked — reported as tool use, which is how a
    /// hook-capable CLI reports an edit it refused.
    pub(super) blocked_markers: Vec<PathBuf>,
    pub(super) steps: Vec<MockStep>,
    pub(super) outcome: MockOutcome,
    pub(super) run_tools_endpoint: Option<RunToolsEndpoint>,
}

/// What ends a played session before its script runs out.
pub(super) struct Stops {
    interrupt: Arc<Notify>,
    kill: Arc<Notify>,
    ends_on_interrupt: bool,
}

impl Stops {
    /// The stops a session scripted with `outcome` answers to. Every
    /// session honors an ordered stop except one scripted to ignore it;
    /// a forced stop ends any of them.
    pub(super) fn of(outcome: &MockOutcome, interrupt: Arc<Notify>, kill: Arc<Notify>) -> Self {
        let ends_on_interrupt = !matches!(
            outcome,
            MockOutcome::Hang {
                on_interrupt: OnInterrupt::Ignore
            }
        );
        Self {
            interrupt,
            kill,
            ends_on_interrupt,
        }
    }

    /// Waits out one step's delay. `false` once a stop arrives first —
    /// the session ends there, mid-script, as a real one does.
    async fn waited(&self, after_ms: u64) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(after_ms)) => true,
            _ = self.kill.notified() => false,
            _ = self.interrupt.notified(), if self.ends_on_interrupt => false,
        }
    }

    /// Waits for a stop and nothing else — what a hung session does.
    async fn awaited(&self) {
        tokio::select! {
            _ = self.kill.notified() => {}
            _ = self.interrupt.notified(), if self.ends_on_interrupt => {}
        }
    }
}

/// Plays `script` into `events` until it runs out, a stop arrives, or the
/// engine stops listening.
pub(super) async fn play(script: Script, events: mpsc::UnboundedSender<AgentEvent>, stops: Stops) {
    // SessionOpened is always the first event, unconditionally.
    if events
        .send(AgentEvent::SessionOpened {
            session_id: script.session_id,
            model: Some(script.model),
        })
        .is_err()
    {
        return;
    }

    for path in script.blocked_markers {
        if events
            .send(AgentEvent::ToolUse {
                name: "edit".to_string(),
                target_digest: format!("blocked:{}", path.display()),
            })
            .is_err()
        {
            return;
        }
    }

    for step in script.steps {
        if !stops.waited(step.after_ms()).await {
            return;
        }
        let event = match played(step, script.run_tools_endpoint.as_ref()).await {
            Ok(event) => event,
            Err(message) => {
                let _ = events.send(AgentEvent::Failed {
                    error: AgentError { message },
                    retryable: false,
                });
                return;
            }
        };
        if events.send(event).is_err() {
            return;
        }
    }

    end(script.outcome, &events, &stops).await;
}

/// The event one step produces, or the message the session fails with
/// because the step did not hold.
async fn played(
    step: MockStep,
    endpoint: Option<&RunToolsEndpoint>,
) -> std::result::Result<AgentEvent, String> {
    Ok(match step {
        MockStep::ToolUse {
            name,
            target_digest,
            ..
        } => AgentEvent::ToolUse {
            name,
            target_digest,
        },
        MockStep::Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
            ..
        } => AgentEvent::Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
        },
        MockStep::Note { text, .. } => AgentEvent::Note { text },
        MockStep::RunTool {
            tool,
            arguments,
            expect,
            ..
        } => return called(tool, arguments, expect, endpoint).await,
    })
}

/// One `run_tool` step: a real MCP call over the wire, judged against
/// what the step says it expects.
///
/// A fixture scripts an intent and the answer it counts on. An accepted
/// call and a refused one the step expected are both audit events and the
/// session goes on — a refusal carries a diagnostic the session acts on.
/// The two mismatches fail the session loudly, because a fixture that got
/// the other answer is asserting something the engine does not do.
async fn called(
    tool: String,
    arguments: serde_json::Map<String, serde_json::Value>,
    expect: ToolExpectation,
    endpoint: Option<&RunToolsEndpoint>,
) -> std::result::Result<AgentEvent, String> {
    match (call_run_tool(endpoint, &tool, arguments).await, expect) {
        (Ok(digest), ToolExpectation::Accepted) => Ok(AgentEvent::ToolUse {
            name: tool,
            target_digest: digest,
        }),
        (Err(error), ToolExpectation::Refused) => Ok(AgentEvent::ToolUse {
            target_digest: yunta_core::sha256_hex(yunta_core::describe(&error).as_bytes())
                .to_string(),
            name: tool,
        }),
        (Err(error), ToolExpectation::Accepted) => Err(yunta_core::describe(&error)),
        (Ok(_), ToolExpectation::Refused) => Err(format!(
            "fixture expected `{tool}` to be refused and it was accepted"
        )),
    }
}

/// How a session that played its whole script ends.
async fn end(outcome: MockOutcome, events: &mpsc::UnboundedSender<AgentEvent>, stops: &Stops) {
    match outcome {
        MockOutcome::Completed { summary } => {
            let _ = events.send(AgentEvent::Completed {
                result: AgentOutcome { summary },
            });
        }
        MockOutcome::Failed { message, retryable } => {
            let _ = events.send(AgentEvent::Failed {
                error: AgentError { message },
                retryable,
            });
        }
        // Both end with no terminal event — a real crash: the
        // engine synthesizes Failed{retryable:true}, not the
        // adapter. Hang additionally waits for a stop before
        // ending, simulating a stuck session a timeout would
        // have to act on.
        MockOutcome::Crash => {}
        MockOutcome::Hang { .. } => stops.awaited().await,
    }
}
