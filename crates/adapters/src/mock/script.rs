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
use yunta_core::fence::Coverage;

use tokio::sync::{mpsc, Notify};
use yunta_core::{ModelName, SessionId};

use super::run_tool::call_run_tool;
use super::{MockOutcome, MockStep, OnInterrupt, ToolExpectation};
use yunta_core::port::RunToolsEndpoint;
use yunta_core::port::{AgentError, AgentEvent, AgentOutcome, FailureCause};

/// One session's script, cut loose from the fixture it was claimed from.
pub(super) struct Script {
    pub(super) session_id: SessionId,
    pub(super) model: ModelName,
    /// The edits this session's effects would have made and the request's
    /// constraints blocked — reported as tool use, which is how a
    /// hook-capable CLI reports an edit it refused.
    pub(super) refused: Vec<PathBuf>,
    pub(super) fence: Option<Coverage>,
    /// The effects this session writes by asking the engine first, as a
    /// CLI's hook does — `None` when they were judged before it opened.
    pub(super) asked: Option<Asked>,
    /// What the session does when its stop is sent back, when it asks
    /// the engine before it stops.
    pub(super) before_stop: Option<BeforeStop>,
    pub(super) steps: Vec<MockStep>,
    pub(super) outcome: MockOutcome,
    pub(super) run_tools_endpoint: Option<RunToolsEndpoint>,
    /// Whether the session lists its run tools before anything else.
    pub(super) lists_tools: bool,
}

/// A session's effects, each written only once the engine allows it.
pub(super) struct Asked {
    pub(super) rail: yunta_core::rail::RailEndpoint,
    pub(super) cwd: PathBuf,
    pub(super) effects: Vec<super::fixture::MockEffect>,
    /// Whether the engine formats what is written: each write that lands
    /// is then told to it, as a CLI's after-write hook does.
    pub(super) formats: bool,
}

/// What a session that asks before it stops does when it is sent back.
pub(super) struct BeforeStop {
    /// Its writes, asked about as a CLI's write hook asks.
    pub(super) writes: Asked,
    pub(super) steps: Vec<MockStep>,
}

impl BeforeStop {
    /// The engine's answer to this session's wish to stop.
    async fn asked(&self, session: &SessionId) -> Result<yunta_core::rail::RailAnswer, String> {
        let call = yunta_core::rail::RailCall {
            session_id: session.clone(),
            act: yunta_core::rail::RailAct::BeforeStop,
        };
        let rail = self.writes.rail.clone();
        tokio::task::spawn_blocking(move || rail.ask(&call, None))
            .await
            .map_err(|e| format!("the stop hook did not finish: {e}"))?
            .map_err(|e| format!("the stop hook could not ask the engine: {e}"))
    }
}

impl Asked {
    /// Asks the engine about each effect and writes the ones it allows.
    async fn written(self, session: SessionId) -> Result<(), String> {
        for effect in self.effects {
            let call = yunta_core::rail::RailCall {
                session_id: session.clone(),
                act: yunta_core::rail::RailAct::BeforeWrite {
                    paths: vec![effect.path.clone()],
                },
            };
            let rail = self.rail.clone();
            let answer =
                tokio::task::spawn_blocking(move || rail.ask(&call, Some(Duration::from_secs(30))))
                    .await
                    .map_err(|e| format!("the write hook did not finish: {e}"))?
                    .map_err(|e| format!("the write hook could not ask the engine: {e}"))?;
            if answer != yunta_core::rail::RailAnswer::Proceed {
                continue;
            }
            super::effects::write_effect(&self.cwd, &effect).map_err(|e| e.to_string())?;
            if self.formats {
                let call = yunta_core::rail::RailCall {
                    session_id: session.clone(),
                    act: yunta_core::rail::RailAct::AfterWrite {
                        paths: vec![effect.path.clone()],
                    },
                };
                let rail = self.rail.clone();
                tokio::task::spawn_blocking(move || rail.ask(&call, Some(Duration::from_secs(30))))
                    .await
                    .map_err(|e| format!("the after-write hook did not finish: {e}"))?
                    .map_err(|e| format!("the after-write hook could not ask the engine: {e}"))?;
            }
        }
        Ok(())
    }
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
    let session = script.session_id.clone();
    if events
        .send(AgentEvent::SessionOpened {
            session_id: script.session_id,
            model: Some(script.model),
            fence: script.fence,
        })
        .is_err()
    {
        return;
    }

    if script.lists_tools {
        if let Some(endpoint) = &script.run_tools_endpoint {
            super::run_tool::list_run_tools(endpoint).await;
        }
    }

    if !wrote(script.asked, session.clone(), script.refused, &events).await {
        return;
    }

    if !stepped(
        script.steps,
        script.run_tools_endpoint.as_ref(),
        &events,
        &stops,
    )
    .await
    {
        return;
    }

    if let (Some(stop), MockOutcome::Completed { .. }) = (script.before_stop, &script.outcome) {
        let endpoint = script.run_tools_endpoint.as_ref();
        if !stopped(stop, session, &events, endpoint, &stops).await {
            return;
        }
    }
    end(script.outcome, &events, &stops).await;
}

/// Asks the engine before the session stops, as a CLI's stop hook does.
/// Sent back, the session does what its script says — once — and asks
/// again; sent back again with nothing left to do, it asks as it is,
/// which the engine answers by letting it go. `false` once the session
/// has failed or gone unheard.
async fn stopped(
    stop: BeforeStop,
    session: SessionId,
    events: &mpsc::UnboundedSender<AgentEvent>,
    endpoint: Option<&RunToolsEndpoint>,
    stops: &Stops,
) -> bool {
    let mut after = Some((stop.writes.effects.clone(), stop.steps.clone()));
    for _ in 0..3 {
        match stop.asked(&session).await {
            Ok(
                yunta_core::rail::RailAnswer::Proceed | yunta_core::rail::RailAnswer::Tell { .. },
            ) => return true,
            Ok(yunta_core::rail::RailAnswer::Refuse { .. }) => {}
            Err(message) => {
                failed(events, message);
                return false;
            }
        }
        let Some((effects, steps)) = after.take() else {
            continue;
        };
        let writes = Asked {
            rail: stop.writes.rail.clone(),
            cwd: stop.writes.cwd.clone(),
            effects,
            formats: stop.writes.formats,
        };
        if let Err(message) = writes.written(session.clone()).await {
            failed(events, message);
            return false;
        }
        if !stepped(steps, endpoint, events, stops).await {
            return false;
        }
    }
    failed(
        events,
        "the engine sent the stop back three times".to_string(),
    );
    false
}

/// Plays `steps`, each after its delay. `false` once a stop arrives first,
/// or the session has failed or gone unheard.
async fn stepped(
    steps: Vec<MockStep>,
    endpoint: Option<&RunToolsEndpoint>,
    events: &mpsc::UnboundedSender<AgentEvent>,
    stops: &Stops,
) -> bool {
    for step in steps {
        if !stops.waited(step.after_ms()).await {
            return false;
        }
        let generated = match played(step, endpoint).await {
            Ok(events) => events,
            Err(message) => {
                failed(events, message);
                return false;
            }
        };
        for event in generated {
            let terminal = matches!(event, AgentEvent::Failed { .. });
            if events.send(event).is_err() || terminal {
                return false;
            }
        }
    }
    true
}

/// The session's writes: the ones it asks the engine about, then the ones
/// judged before it opened that were refused. `false` once the session
/// has ended, failed or unheard.
async fn wrote(
    asked: Option<Asked>,
    session: SessionId,
    refused: Vec<PathBuf>,
    events: &mpsc::UnboundedSender<AgentEvent>,
) -> bool {
    if let Some(asked) = asked {
        if let Err(message) = asked.written(session).await {
            failed(events, message);
            return false;
        }
    }
    refused.iter().all(|path| {
        events
            .send(AgentEvent::WriteRefused {
                target: yunta_core::events::ToolTarget::of_path(path),
            })
            .is_ok()
    })
}

/// Ends the session failed with `message`. A receiver that is gone has
/// stopped reading already, and there is nobody left to tell.
fn failed(events: &mpsc::UnboundedSender<AgentEvent>, message: String) {
    if events
        .send(AgentEvent::Failed {
            error: AgentError::message(message),
            cause: FailureCause::Final,
        })
        .is_err()
    {
        tracing::debug!("a mock session failed after its reader left");
    }
}

/// The event one step produces, or the message the session fails with
/// because the step did not hold.
async fn played(
    step: MockStep,
    endpoint: Option<&RunToolsEndpoint>,
) -> std::result::Result<Vec<AgentEvent>, String> {
    Ok(vec![match step {
        MockStep::ToolUse { name, target, .. } => AgentEvent::ToolUse {
            name,
            target: yunta_core::events::ToolTarget::opaque(target.as_bytes()),
        },
        MockStep::Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
            ..
        } => AgentEvent::Usage {
            input_tokens: Some(input_tokens),
            output_tokens: Some(output_tokens),
            cached_input_tokens,
        },
        MockStep::Note { text, .. } => AgentEvent::Note { text },
        MockStep::RunToolsMounted { count, .. } => AgentEvent::RunToolsMounted { count },
        MockStep::RunTool {
            tool,
            arguments,
            expect,
            ..
        } => return called(tool, arguments, expect, endpoint).await,
    }])
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
) -> std::result::Result<Vec<AgentEvent>, String> {
    match (call_run_tool(endpoint, &tool, arguments).await, expect) {
        (Ok(answer), ToolExpectation::Accepted) => Ok(vec![AgentEvent::ToolUse {
            name: tool,
            target: yunta_core::events::ToolTarget::opaque(answer.as_bytes()),
        }]),
        (Err(error), ToolExpectation::Refused) if error.reached_call() => {
            let mut events = Vec::new();
            if let Some(known) = yunta_core::RunTool::parse(&tool) {
                events.push(AgentEvent::RunToolFailed {
                    tool: known,
                    cause: yunta_core::events::RunToolFailureCause::CallFailed,
                });
            }
            events.push(AgentEvent::ToolUse {
                target: yunta_core::events::ToolTarget::opaque(tool.as_bytes()),
                name: tool,
            });
            Ok(events)
        }
        (Err(error), _) => {
            let mut events = Vec::new();
            let known = yunta_core::RunTool::parse(&tool);
            if error.reached_call() {
                if let Some(known) = known {
                    events.push(AgentEvent::RunToolFailed {
                        tool: known,
                        cause: yunta_core::events::RunToolFailureCause::CallFailed,
                    });
                }
            }
            let message = known.map_or_else(
                || "run tool call failed".to_string(),
                |known| format!("run tool `{}` failed", known.name()),
            );
            events.push(AgentEvent::Failed {
                error: AgentError::message(message),
                cause: FailureCause::Final,
            });
            Ok(events)
        }
        (Ok(_), ToolExpectation::Refused) => {
            Err("fixture expected a refused run tool call and it was accepted".to_string())
        }
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
            // Read the way a real adapter reads its CLI's message, so a
            // fixture that prints what a cut-off CLI prints is one.
            let cause = match crate::failure::classify(&message) {
                crate::failure::FailureKind::Unreachable => FailureCause::Unreachable,
                _ if retryable => FailureCause::Retryable,
                _ => FailureCause::Final,
            };
            let _ = events.send(AgentEvent::Failed {
                error: AgentError::message(message),
                cause,
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
