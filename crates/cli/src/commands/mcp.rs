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

use std::path::Path;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Value};
use yunta_core::{AdapterId, ArtifactKind, Clock, ModeName, RunId};

use crate::context::Context;
use crate::error::{CliError, Outcome};

pub async fn mcp() -> Result<Outcome, CliError> {
    let transport = rmcp::transport::io::stdio();
    let server = YuntaMcpServer
        .serve(transport)
        .await
        .map_err(|e| CliError::msg(format!("cannot start the MCP server: {e}")))?;
    server
        .waiting()
        .await
        .map_err(|e| CliError::msg(format!("MCP server exited abnormally: {e}")))?;
    Ok(Outcome::Success)
}

#[derive(Clone)]
struct YuntaMcpServer;

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
            "list_workflows" => tool_list_workflows(&cwd).await,
            "workflow_status" => tool_workflow_status(&cwd, &args).await,
            "run_workflow" => tool_run_workflow(&cwd, &args).await,
            "resume_run" => tool_resume_run(&cwd, &args).await,
            "resolve_gate" => tool_resolve_gate(&cwd, &args).await,
            "answer_questions" => tool_answer_questions(&cwd, &args).await,
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

fn required_str<'a>(
    args: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<&'a str, CliError> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::msg(format!("missing or non-string argument `{name}`")))
}

/// The `run_id` argument as the id it has to be, so a call naming
/// something that is not one is answered before any disk is read.
fn required_run_id(args: &serde_json::Map<String, Value>) -> Result<RunId, CliError> {
    Ok(required_str(args, "run_id")?.parse()?)
}

async fn tool_list_workflows(cwd: &Path) -> Result<String, CliError> {
    // The same catalog `yunta list` renders, built in-process: shelling
    // out to a subprocess would print onto this server's own stdout — the
    // very stream its JSON-RPC replies travel on. Best-effort storage, so
    // a repo with no state root yet still lists, just without estimates.
    let ctx = Context::resolve_in(cwd.to_path_buf()).ok();
    Ok(super::list::render_catalog(cwd, ctx.as_ref()).await)
}

async fn tool_workflow_status(
    cwd: &Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let ctx = Context::resolve_in(cwd.to_path_buf())?;
    let open = ctx.open_run(&run_id).await?;
    let (events, manifest) = (open.events, open.manifest.doc);
    // The same versioned DTO `yunta status --json` prints, serialized to
    // the tool result rather than to stdout, and read at this server's
    // own injected clock.
    crate::json::to_json_string(&crate::json::RunDocument::of(
        &run_id,
        &events,
        &manifest,
        ctx.clock.now(),
    ))
    .map_err(CliError::msg)
}

async fn tool_run_workflow(
    cwd: &Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, CliError> {
    let name = required_str(args, "workflow")?;
    let mut inputs: Vec<String> = Vec::new();
    if let Some(object) = args.get("inputs").and_then(Value::as_object) {
        for (key, value) in object {
            let rendered = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            inputs.push(format!("{key}={rendered}"));
        }
    }
    let adapter = args
        .get("adapter")
        .and_then(Value::as_str)
        .map(str::parse::<AdapterId>)
        .transpose()?;
    let mode = args
        .get("mode")
        .and_then(Value::as_str)
        .map(str::parse::<ModeName>)
        .transpose()?;

    // Resolves `name` against the whole catalog — the repo's own
    // `.yunta/workflows/` then a publisher's vendored packs
    // (`acme/review`) — then creates the run and hands it off, all
    // in-process through the same `start_detached` `yunta run --detach`
    // calls.
    let ctx = Context::resolve_in(cwd.to_path_buf())?;
    let storage = ctx.async_storage().await?;
    let started = super::run::start_detached(
        &ctx,
        &storage,
        Path::new(name),
        &inputs,
        adapter.as_ref(),
        mode.as_ref(),
    )
    .await?;
    // The run id first, so a client that reads one line still reads the
    // thing it asked for, and §8.6's warning under it when this
    // workflow's history has one: a client that starts runs is the one
    // deciding whether a cap is worth starting under, and stderr never
    // reaches it.
    Ok(match started.budget_warning {
        Some(warning) => format!("run_id: {}\n{warning}", started.run_id),
        None => format!("run_id: {}", started.run_id),
    })
}

async fn tool_resume_run(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let ctx = Context::resolve_in(cwd.to_path_buf())?;
    let run_dir = ctx.open_run(&run_id).await?.run_dir;
    super::spawn_detached_resume(&run_dir, run_id.as_str(), cwd)
        .await
        .map_err(|source| super::DetachedResumeError::new(&run_id, source))?;
    Ok(format!(
        "run {run_id}: resumed, driving forward independently"
    ))
}

async fn tool_resolve_gate(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let option = required_str(args, "option")?.parse::<yunta_core::OptionId>()?;
    let by = args
        .get("by")
        .and_then(Value::as_str)
        .map(str::parse::<yunta_core::Responder>)
        .transpose()?;
    let text = args.get("text").and_then(Value::as_str).map(str::to_string);

    // The command's own path, called rather than copied: a client that
    // answers a gate records exactly what a person answering it records,
    // under this server's own injected clock, and reads back the same
    // sentence.
    let ctx = Context::resolve_in(cwd.to_path_buf())?;
    super::resolve_gate::resolve(&ctx, &run_id, &option, by.as_ref(), text.as_deref()).await
}

async fn tool_answer_questions(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, CliError> {
    let run_id = required_run_id(args)?;
    let node = required_str(args, "node")?.parse::<yunta_core::NodeId>()?;
    let by = args
        .get("by")
        .and_then(Value::as_str)
        .map(str::parse::<yunta_core::Responder>)
        .transpose()?;
    let answers = answers_from(args)?;

    let ctx = Context::resolve_in(cwd.to_path_buf())?;
    let open = ctx.open_run(&run_id).await?;
    let (run_dir, manifest) = (open.run_dir, open.manifest.doc);
    let storage = ctx.async_storage().await?;

    // The engine's own door, under this server's injected clock: the
    // round is re-read from the document the node asked from, the reply
    // is judged against it, and either both the acceptance and the
    // `questions_answered` land or neither does.
    yunta_engine::answer_questions(
        &manifest,
        &storage,
        &run_id,
        &run_dir,
        &ctx.clock,
        &node,
        yunta_engine::AnswersReply {
            answers,
            channel: yunta_core::events::Channel::Mcp,
            responder: Some(crate::identity::responder(by.as_ref())),
        },
    )
    .await?;

    // The answers are on the log; a detached process carries the node
    // on from there, exactly as it does after a gate is resolved.
    super::spawn_detached_resume(&run_dir, run_id.as_str(), cwd)
        .await
        .map_err(|source| CliError::AnswersRecordedNotResumed {
            source: super::DetachedResumeError::new(&run_id, source),
        })?;
    Ok(format!(
        "run {run_id}: node `{node}` answered, driving forward independently"
    ))
}

/// The `answers` argument as the engine takes it: one entry per
/// question, each naming the question it answers.
///
/// Only the shape of the argument is checked here — that it is a list
/// of `{id, value}` with an id that is one. Whether those answers
/// satisfy the questions is the document's own judgement, and it is
/// made once, inside the engine, so this surface and the console reach
/// the same verdict.
fn answers_from(
    args: &serde_json::Map<String, Value>,
) -> Result<Vec<yunta_core::Answer>, CliError> {
    let Some(entries) = args.get("answers").and_then(Value::as_array) else {
        return Err(CliError::msg(
            "`answers` is required: a list of `{id, value}`, one per question",
        ));
    };
    entries
        .iter()
        .map(|entry| {
            let id = entry
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::msg("every `answers` entry names the `id` it answers"))?
                .parse::<yunta_core::QuestionId>()?;
            // A value arrives as whatever JSON the client had for it
            // and is recorded as the text the document is written in —
            // the same text a person types at the console, so a `3`
            // and a "3" are one answer.
            let value = match entry.get("value") {
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => return Err(CliError::msg(format!("answer `{id}` names no `value`"))),
            };
            Ok(yunta_core::Answer { id, value })
        })
        .collect()
}
