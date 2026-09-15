//! `yunta mcp`: the control-plane MCP server, stdio. Seven tools —
//! `document_shape`, `list_workflows`, `run_workflow`,
//! `workflow_status`, `resume_run`, `resolve_gate`, `answer_questions`
//! — none of which ever blocks for a run's own duration:
//! `run_workflow`/`resume_run` hand off to a detached `yunta resume`
//! and return immediately; a caller tracks progress by polling
//! `workflow_status` (pull, never push — the same model external gates
//! already use).
//!
//! `list_workflows`/`workflow_status` reuse `yunta list`/`yunta status`
//! verbatim by shelling out to this same binary — their rendering is
//! already the structured text those commands produce, and a second
//! copy of that rendering here would drift. `resume_run`/
//! `resolve_gate`/`answer_questions` call the already-factored,
//! already-tested primitives directly in-process
//! (`spawn_detached_resume`, `commands::resolve_gate::resolve`,
//! `yunta_engine::answer_questions`) rather than round-tripping through
//! a subprocess for no reason. Each of the last two is the second
//! surface of a door a run already answers through, never a second
//! implementation of it.
//!
//! Tool descriptions are written to help a client *decide*, not merely
//! to describe — each names when reaching for a verified workflow
//! beats implementing directly.

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Value};
use yunta_core::ArtifactKind;

use crate::error::{CliError, Outcome};

mod tools;
use tools::{
    tool_answer_questions, tool_list_workflows, tool_resolve_gate, tool_resume_run,
    tool_run_workflow, tool_workflow_status,
};

pub async fn mcp() -> Result<Outcome, CliError> {
    // One interruption per process, installed before the first request
    // can arrive: every `Context` a tool builds shares its two stages,
    // so a SIGINT stops what the server is doing and closes the server.
    let interrupt =
        std::sync::Arc::new(crate::interrupt::Interrupt::ctrl_c().map_err(|source| {
            CliError::io(
                "install the interrupt handler for",
                "the MCP server",
                source,
            )
        })?);
    let stopping = interrupt.stop().clone();
    let transport = rmcp::transport::io::stdio();
    let server = YuntaMcpServer { interrupt }
        .serve(transport)
        .await
        .map_err(|e| CliError::msg(format!("cannot start the MCP server: {e}")))?;
    tokio::select! {
        ended = server.waiting() => {
            ended.map_err(|e| CliError::msg(format!("MCP server exited abnormally: {e}")))?;
        }
        // The interrupt closes the server the way the process exiting
        // used to: nothing is left half-answered on stdout.
        () = stopping.cancelled() => {}
    }
    Ok(Outcome::Success)
}

#[derive(Clone)]
struct YuntaMcpServer {
    interrupt: std::sync::Arc<crate::interrupt::Interrupt>,
}

impl ServerHandler for YuntaMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(yunta_engine::mcp::tool_list(tool_definitions()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        let args = request.arguments.unwrap_or_default();
        let cwd = match std::env::current_dir() {
            Ok(cwd) => cwd,
            Err(e) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "cannot determine the current directory: {e}"
                ))])
                .into());
            }
        };
        let outcome = match request.name.as_ref() {
            "list_workflows" => tool_list_workflows(&cwd, self.interrupt.shared()).await,
            "workflow_status" => tool_workflow_status(&cwd, &args, self.interrupt.shared()).await,
            "run_workflow" => tool_run_workflow(&cwd, &args, self.interrupt.shared()).await,
            "resume_run" => tool_resume_run(&cwd, &args, self.interrupt.shared()).await,
            "resolve_gate" => tool_resolve_gate(&cwd, &args, self.interrupt.shared()).await,
            "answer_questions" => tool_answer_questions(&cwd, &args, self.interrupt.shared()).await,
            "document_shape" => tool_document_shape(&args),
            // An unknown tool is a protocol error, not a tool that ran
            // and failed — the client asked for something this server
            // never advertised.
            other => {
                return Err(McpError::invalid_params(
                    format!("unknown tool `{other}`"),
                    None,
                ))
            }
        };
        Ok(tool_result(outcome))
    }
}

/// The one adapter from a tool's own result to the protocol's.
///
/// A tool that ran and failed answers with the sentence [`CliError`]
/// writes, which is the sentence the equivalent command prints on
/// stderr — so an agent client asking this server gets the advice a
/// person at a terminal gets, rather than a second phrasing of the same
/// refusal.
fn tool_result(outcome: Result<String, CliError>) -> rmcp::model::CallToolResponse {
    match outcome {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]).into(),
        Err(refusal) => CallToolResult::error(vec![ContentBlock::text(refusal.to_string())]).into(),
    }
}

fn empty_schema() -> serde_json::Map<String, Value> {
    json!({"type": "object", "properties": {}})
        .as_object()
        .cloned()
        .unwrap_or_default()
}

/// The shapes a client can ask for, as the tool's own enum — so the
/// catalog is visible the moment a client connects, before it calls
/// anything.
fn document_kinds() -> Vec<&'static str> {
    ArtifactKind::ALL.iter().map(|kind| kind.as_str()).collect()
}

/// The shape of one document, verbatim from the constant every other
/// door publishes.
///
/// The kind is parsed by `ArtifactKind`'s own `FromStr`, so this tool
/// and `yunta schema` answer an unknown kind with the same sentence.
fn tool_document_shape(args: &serde_json::Map<String, Value>) -> Result<String, CliError> {
    let Some(name) = args.get("kind").and_then(Value::as_str) else {
        return Err(CliError::msg(format!(
            "`kind` is required: one of {}",
            ArtifactKind::listed()
        )));
    };
    let kind = name.parse::<ArtifactKind>()?;
    Ok(yunta_core::shape::contract(kind))
}

fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool::new(
            "document_shape",
            "The exact shape of a document Yunta reads and validates. Call this BEFORE \
             writing a tasks document, a findings artifact or a questions artifact — they are \
             validated strictly, a key that is not in the shape fails the node that produced \
             it, and there is no other way to learn the format. Returns a complete, valid \
             example with every field annotated.",
            json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": document_kinds(),
                        "description": "Which document to describe.",
                    }
                },
                "required": ["kind"],
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        ),
        Tool::new(
            "list_workflows",
            "Lists this repo's own catalog of workflows (name, description, declared \
             inputs) — call this BEFORE writing ad-hoc code for a task a workflow might \
             already cover verified. A workflow's criteria and scope checks give a \
             mechanical guarantee free-form edits never do.",
            empty_schema(),
        ),
        Tool::new(
            "run_workflow",
            "Starts a workflow by its catalog name (from list_workflows) and returns a \
             run_id immediately — it never blocks for the run's own duration, however \
             long that is. Poll workflow_status(run_id) to track progress; prefer this \
             over ad-hoc scripting whenever the task matches a workflow's own shape.",
            json!({
                "type": "object",
                "properties": {
                    "workflow": {"type": "string", "description": "catalog name, as listed by list_workflows"},
                    "inputs": {"type": "object", "description": "declared input name -> value", "additionalProperties": {"type": "string"}},
                    "adapter": {"type": "string", "description": "override runners: resolution"},
                    "mode": {"type": "string", "description": "workflow mode; omit for the floor mode"},
                },
                "required": ["workflow"],
            })
            .as_object().cloned().unwrap_or_default(),
        ),
        Tool::new(
            "workflow_status",
            "Reads a run's current derived state (nodes, tasks, tokens, waiting/paused \
             reason) straight from its event log — the only way to know whether a \
             run_workflow/resume_run/resolve_gate call actually finished. A run parked \
             on a decision carries it under `decision`: the node, every option id with \
             its tradeoff, and the command that answers it.",
            json!({
                "type": "object",
                "properties": {"run_id": {"type": "string"}},
                "required": ["run_id"],
            })
            .as_object().cloned().unwrap_or_default(),
        ),
        Tool::new(
            "resume_run",
            "Hands a paused run back to a detached process and returns immediately — use \
             after external state changed (an approval landed, a file was fixed by hand) \
             with nothing else for resolve_gate to answer.",
            json!({
                "type": "object",
                "properties": {"run_id": {"type": "string"}},
                "required": ["run_id"],
            })
            .as_object().cloned().unwrap_or_default(),
        ),
        Tool::new(
            "resolve_gate",
            "Answers a paused run's decision — call workflow_status first to read \
             the run's own pause reason and menu of options. Covers exhausted re-routes \
             (retry/abort/promote) and unresolved gate nodes; the decision is recorded on \
             the run's log and a detached process applies it, so this returns \
             immediately — poll workflow_status to see the outcome.",
            json!({
                "type": "object",
                "properties": {
                    "run_id": {"type": "string"},
                    "option": {"type": "string", "description": "the chosen option id"},
                    "by": {"type": "string", "description": "who's answering, for the audit trail"},
                    "text": {"type": "string", "description": "free-form context alongside the choice"},
                },
                "required": ["run_id", "option"],
            })
            .as_object().cloned().unwrap_or_default(),
        ),
        Tool::new(
            "answer_questions",
            "Answers the questions a paused node asked — call workflow_status first to \
             read them, each with its id. Every question the document requires has to be \
             answered, and a value has to be one its type accepts; a reply that is not is \
             refused whole, naming what is wrong, with nothing recorded. The answers are \
             recorded on the run's log and a detached process carries the node on, so this \
             returns immediately — poll workflow_status to see the outcome.",
            json!({
                "type": "object",
                "properties": {
                    "run_id": {"type": "string"},
                    "node": {"type": "string", "description": "the node that asked"},
                    "answers": {
                        "type": "array",
                        "description": "one entry per question, by the id the question carries",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string"},
                                "value": {"description": "the answer, in the shape the question's type asks for"},
                            },
                            "required": ["id", "value"],
                        },
                    },
                    "by": {"type": "string", "description": "who's answering, for the audit trail"},
                },
                "required": ["run_id", "node", "answers"],
            })
            .as_object().cloned().unwrap_or_default(),
        ),
    ]
}
