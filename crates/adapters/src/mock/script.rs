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
//! What the session says goes out the way its CLI prints it: each step is
//! rendered as lines in that CLI's own format and read back by the reader
//! the engine uses for that CLI, and every event read passes the stream
//! guard, so the stream the engine sees is the one a real session makes.
//!
//! Every send is checked: a receiver that is gone means the engine
//! stopped reading this session, and a script that keeps playing into a
//! closed channel is work nobody asked for.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Notify};
use yunta_core::events::{FailureCause, RefusalCause, ToolTarget};
use yunta_core::port::{AgentError, AgentEvent, RunToolsEndpoint};
use yunta_core::process::stream::StreamGuard;
use yunta_core::process::subprocess::LineParser;
use yunta_core::rail::{RailAnswer, RailEvent};
use yunta_core::{ModelName, RunTool, SessionId, ToolNaming};

use super::effects::{ask_hook, Asked};
use super::run_tool::{call_run_tool, list_run_tools, RunToolCallError};
use super::{MockOutcome, MockStep, OnInterrupt, ToolExpectation, ID};
use crate::dialect::DialectName;

/// One session's script, cut loose from the fixture it was claimed from.
pub(super) struct Script {
    pub(super) dialect: DialectName,
    pub(super) session_id: SessionId,
    pub(super) model: ModelName,
    /// Reads the lines the session's CLI prints, as the engine reads that
    /// CLI's stream.
    pub(super) reader: Box<dyn LineParser>,
    /// How the session's CLI names the run tools it calls.
    pub(super) naming: ToolNaming,
    /// How long one run tool call may take: the CLI's own call timeout.
    pub(super) within: Duration,
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

/// What a session that asks before it stops does when it is sent back.
pub(super) struct BeforeStop {
    /// Its writes, asked about as a CLI's write hook asks.
    pub(super) writes: Asked,
    pub(super) steps: Vec<MockStep>,
}

impl BeforeStop {
    /// What the CLI reads from the engine's reply to this session's wish to
    /// stop. A stop names no path, so the hook is asked with an empty one.
    async fn asked(&self, session: &SessionId) -> Result<RailAnswer, String> {
        ask_hook(
            self.writes.dialect,
            &self.writes.rail,
            RailEvent::BeforeStop,
            session,
            Path::new(""),
        )
        .await
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

/// What a played session says on its stream, and the calls it makes on the
/// engine's run tools endpoint. Every event passes the stream guard before
/// it is sent.
struct Stream {
    dialect: DialectName,
    naming: ToolNaming,
    endpoint: Option<RunToolsEndpoint>,
    /// How long one run tool call may take before the CLI gives up on it.
    within: Duration,
    reader: Box<dyn LineParser>,
    guard: StreamGuard,
    events: mpsc::UnboundedSender<AgentEvent>,
    /// The usage steps played so far. A CLI that reports usage at the end of
    /// its turn carries their sum on the line that closes the session.
    usage: Vec<MockStep>,
}

impl Stream {
    /// Sends one event the guard admits. `true` while the session may go
    /// on: the event went out, it was not a terminal one, and the engine
    /// still reads.
    fn send(&mut self, event: AgentEvent) -> bool {
        let Some(event) = self.guard.admit(event, ID.as_str()) else {
            return false;
        };
        let terminal = matches!(
            event,
            AgentEvent::Completed { .. } | AgentEvent::Failed { .. }
        );
        self.events.send(event).is_ok() && !terminal
    }

    /// Sends `events` in order, stopping at the first that does not go on.
    fn emit(&mut self, events: impl IntoIterator<Item = AgentEvent>) -> bool {
        events.into_iter().all(|event| self.send(event))
    }

    /// Reads the lines a CLI printed, sending each event they say.
    fn read(&mut self, lines: Vec<String>) -> bool {
        lines.iter().all(|line| {
            let events = self.reader.parse(line);
            self.emit(events)
        })
    }

    /// Prints one step the way its CLI prints it, and reads the lines back. A
    /// usage step is kept as well, for the line that closes the session.
    fn say(&mut self, step: MockStep) -> bool {
        if matches!(step, MockStep::Usage { .. }) {
            self.usage.push(step.clone());
        }
        let lines = self.dialect.render(&step, self.naming);
        self.read(lines)
    }

    /// Ends the session failed with `message`.
    fn fail(&mut self, message: String) -> bool {
        self.send(AgentEvent::Failed {
            error: AgentError::message(message),
            cause: FailureCause::Refused(RefusalCause::Final),
        });
        false
    }

    /// Plays one step. A run tool step calls the engine first, and a rejected
    /// one is reported without a call; any other step is printed as its CLI
    /// prints it.
    async fn step(&mut self, step: MockStep) -> bool {
        match step {
            MockStep::RunTool {
                tool,
                arguments,
                expect,
                ..
            } => self.run_tool(tool, arguments, expect).await,
            MockStep::RunToolRejected { tool, .. } => {
                self.send(AgentEvent::RunToolRejected { tool })
            }
            step => self.say(step),
        }
    }

    /// One run tool step: a real MCP call, judged against what the step
    /// expects. A refusal the step expects is audited and the session goes
    /// on. The other mismatches fail it, because a fixture that got the
    /// other answer is asserting something the engine does not do.
    async fn run_tool(
        &mut self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        expect: ToolExpectation,
    ) -> bool {
        let called = call_run_tool(self.endpoint.as_ref(), &tool, arguments, self.within).await;
        match (called, expect) {
            (Ok(answer), ToolExpectation::Accepted) => {
                // The call is the CLI's own tool use, named the way its CLI
                // names the tool.
                let name = RunTool::parse(&tool)
                    .map_or_else(|| tool.clone(), |known| known.called(self.naming));
                self.say(MockStep::ToolUse {
                    name,
                    target: answer,
                    after_ms: 0,
                })
            }
            // The engine answered the call, with a refusal or a protocol error:
            // the CLI reports the rejection, and the session takes it and goes on.
            (
                Err(RunToolCallError::Refused { .. } | RunToolCallError::Call { .. }),
                ToolExpectation::Refused,
            ) => {
                let mut events = call_rejection(&tool);
                events.push(AgentEvent::ToolUse {
                    target: ToolTarget::opaque(tool.as_bytes()),
                    name: tool,
                });
                self.emit(events)
            }
            (Err(_), _) => {
                let mut events = call_rejection(&tool);
                let message = RunTool::parse(&tool).map_or_else(
                    || "run tool call failed".to_string(),
                    |known| format!("run tool `{}` failed", known.name()),
                );
                events.push(AgentEvent::Failed {
                    error: AgentError::message(message),
                    cause: FailureCause::Refused(RefusalCause::Final),
                });
                self.emit(events);
                false
            }
            (Ok(_), ToolExpectation::Refused) => self
                .fail("fixture expected a refused run tool call and it was accepted".to_string()),
        }
    }
}

/// What a failed call leaves in the stream for the run tool it named: the
/// rejection its CLI reports. Whether the call reached the engine is for the
/// engine to read from its own listener.
fn call_rejection(tool: &str) -> Vec<AgentEvent> {
    RunTool::parse(tool)
        .map(|known| AgentEvent::RunToolRejected { tool: known })
        .into_iter()
        .collect()
}

/// The session's opening: the lines its CLI prints before it opens, then the
/// opening line itself and the run tool listing. `false` once the session
/// has ended.
async fn opening(
    stream: &mut Stream,
    stops: &Stops,
    before_open: Vec<MockStep>,
    session: &SessionId,
    model: &ModelName,
    lists_tools: bool,
) -> bool {
    // A line the CLI prints before its session opens goes out first, and the
    // stream guard refuses it there: nothing after it is read.
    if !stepped(before_open, stream, stops).await {
        return false;
    }
    // Otherwise SessionOpened is the first event: the CLI's own opening line,
    // read back the way the engine reads that CLI.
    if !stream.read(stream.dialect.render_open(session, Some(model))) {
        return false;
    }
    if let (true, Some(endpoint)) = (lists_tools, &stream.endpoint) {
        list_run_tools(endpoint, stream.within).await;
    }
    true
}

/// Plays `script` into `events` until it runs out, a stop arrives, or the
/// engine stops listening.
pub(super) async fn play(script: Script, events: mpsc::UnboundedSender<AgentEvent>, stops: Stops) {
    let Script {
        dialect,
        session_id,
        model,
        reader,
        naming,
        within,
        run_tools_endpoint,
        lists_tools,
        asked,
        before_stop,
        steps,
        outcome,
    } = script;
    let mut stream = Stream {
        dialect,
        naming,
        endpoint: run_tools_endpoint,
        within,
        reader,
        guard: StreamGuard::default(),
        events,
        usage: Vec::new(),
    };
    let (before_open, steps): (Vec<MockStep>, Vec<MockStep>) = steps
        .into_iter()
        .partition(|step| matches!(step, MockStep::BeforeOpen { .. }));
    if !opening(
        &mut stream,
        &stops,
        before_open,
        &session_id,
        &model,
        lists_tools,
    )
    .await
    {
        return;
    }
    if !wrote(asked, &session_id, &mut stream).await {
        return;
    }
    if !stepped(steps, &mut stream, &stops).await {
        return;
    }
    if !stop_before_end(before_stop, &outcome, &session_id, &mut stream, &stops).await {
        return;
    }
    end(outcome, &mut stream, &stops).await;
}

/// Runs the stop a completed session's script asks for before the session
/// ends. `false` once the session has ended or the engine stopped listening.
async fn stop_before_end(
    before_stop: Option<BeforeStop>,
    outcome: &MockOutcome,
    session: &SessionId,
    stream: &mut Stream,
    stops: &Stops,
) -> bool {
    match (before_stop, outcome) {
        (Some(stop), MockOutcome::Completed { .. }) => stopped(stop, session, stream, stops).await,
        _ => true,
    }
}

/// The session's writes that it asks the engine about. `false` once the
/// session has ended, failed or unheard.
async fn wrote(asked: Option<Asked>, session: &SessionId, stream: &mut Stream) -> bool {
    match asked {
        Some(asked) => match asked.written(session).await {
            Ok(()) => true,
            Err(message) => stream.fail(message),
        },
        None => true,
    }
}

/// Asks the engine before the session stops, as a CLI's stop hook does.
/// Sent back, the session does what its script says — once — and asks
/// again; sent back again with nothing left to do, it asks as it is,
/// which the engine answers by letting it go. `false` once the session
/// has failed or gone unheard.
async fn stopped(
    stop: BeforeStop,
    session: &SessionId,
    stream: &mut Stream,
    stops: &Stops,
) -> bool {
    let mut after = Some((stop.writes.effects.clone(), stop.steps.clone()));
    for _ in 0..3 {
        match stop.asked(session).await {
            Ok(RailAnswer::Proceed | RailAnswer::Tell { .. }) => return true,
            Ok(RailAnswer::Refuse { .. }) => {}
            Err(message) => return stream.fail(message),
        }
        let Some((effects, steps)) = after.take() else {
            continue;
        };
        let writes = Asked {
            dialect: stop.writes.dialect,
            rail: stop.writes.rail.clone(),
            cwd: stop.writes.cwd.clone(),
            effects,
            formats: stop.writes.formats,
        };
        if let Err(message) = writes.written(session).await {
            return stream.fail(message);
        }
        if !stepped(steps, stream, stops).await {
            return false;
        }
    }
    stream.fail("the engine sent the stop back three times".to_string())
}

/// Plays `steps`, each after its delay. `false` once a stop arrives first,
/// or the session has failed or gone unheard.
async fn stepped(steps: Vec<MockStep>, stream: &mut Stream, stops: &Stops) -> bool {
    for step in steps {
        if !stops.waited(step.after_ms()).await {
            return false;
        }
        if !stream.step(step).await {
            return false;
        }
    }
    true
}

/// How a session that played its whole script ends. A completed or failed
/// session closes with the line its CLI closes with, read back the way the
/// engine reads that CLI, so the outcome and its cause come from the reader
/// a real session's come from.
async fn end(outcome: MockOutcome, stream: &mut Stream, stops: &Stops) {
    match &outcome {
        MockOutcome::Completed { .. } | MockOutcome::Failed { .. } => {
            let lines = stream.dialect.render_close(&outcome, &stream.usage);
            stream.read(lines);
        }
        // Crash ends with no terminal event at all — a real crash: the engine
        // synthesizes the failure, not the adapter. Hang additionally waits
        // for a stop before ending, simulating a stuck session a timeout
        // would have to act on.
        MockOutcome::Crash => {}
        MockOutcome::Hang { .. } => stops.awaited().await,
    }
}
