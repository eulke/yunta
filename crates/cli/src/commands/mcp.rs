//! `yunta mcp`: the control-plane MCP server, stdio. Five tools —
//! `list_workflows`, `run_workflow`, `workflow_status`, `resume_run`,
//! `resolve_gate` — none of which ever blocks for a run's own
//! duration: `run_workflow`/`resume_run` hand off to a detached
//! `yunta resume` and return immediately; a caller tracks progress by
//! polling `workflow_status` (pull, never push — the same model
//! external gates already use).
//!
//! `list_workflows`/`workflow_status` reuse `yunta list`/`yunta status`
//! verbatim by shelling out to this same binary — their rendering is
//! already the structured text those commands produce, and a second
//! copy of that rendering here would drift. `resume_run`/
//! `resolve_gate` call the already-factored, already-tested primitives
//! directly in-process (`spawn_detached_resume`, `yunta_engine::
//! resolve_gate`) rather than round-tripping through a subprocess for
//! no reason.
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
use yunta_core::{AdapterId, ArtifactKind, Clock, Manifest, ModeName, RunId};

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
        Ok(ListToolsResult {
            tools: tool_definitions(),
            ..Default::default()
        })
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
        Ok(match outcome {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]).into(),
            Err(text) => CallToolResult::error(vec![ContentBlock::text(text)]).into(),
        })
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
fn tool_document_shape(args: &serde_json::Map<String, Value>) -> Result<String, String> {
    let Some(name) = args.get("kind").and_then(Value::as_str) else {
        return Err(format!(
            "`kind` is required: one of {}",
            ArtifactKind::listed()
        ));
    };
    let kind = name.parse::<ArtifactKind>().map_err(|e| e.to_string())?;
    Ok(yunta_core::shape::published(kind).to_string())
}

fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool::new(
            "document_shape",
            "The exact shape of a document Yunta reads and validates. Call this BEFORE \
             writing a task ledger, a findings artifact or a questions artifact — they are \
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
    ]
}

fn required_str<'a>(
    args: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or non-string argument `{name}`"))
}

async fn tool_list_workflows(cwd: &Path) -> Result<String, String> {
    // The same catalog `yunta list` renders, built in-process: shelling
    // out to a subprocess would print onto this server's own stdout — the
    // very stream its JSON-RPC replies travel on. Best-effort storage, so
    // a repo with no state root yet still lists, just without estimates.
    let history_source = Context::resolve_in(cwd.to_path_buf()).ok().and_then(|ctx| {
        let storage = ctx.storage().ok()?;
        Some((ctx.project, storage))
    });
    Ok(super::list::render_catalog(cwd, history_source.as_ref()))
}

async fn tool_workflow_status(
    cwd: &Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    let run_id: RunId = required_str(args, "run_id")?
        .parse()
        .map_err(|e| format!("{e}"))?;
    let ctx = Context::resolve_in(cwd.to_path_buf()).map_err(|e| e.to_string())?;
    let storage = ctx.async_storage().await.map_err(|e| e.to_string())?;
    let events = storage
        .events_for_run(run_id.clone())
        .await
        .map_err(|e| e.to_string())?;
    if events.is_empty() {
        return Err(format!(
            "no run `{run_id}` in {}",
            ctx.project.storage_path.display()
        ));
    }
    let manifest_path = ctx
        .project
        .run_dir(run_id.as_str())
        .unwrap_or_else(|| ctx.project.runs_root.join(run_id.as_str()))
        .join("manifest.yaml");
    let manifest: Manifest =
        crate::load_yaml(&manifest_path, "run manifest").map_err(|e| e.to_string())?;
    // The same versioned DTO `yunta status --json` prints, serialized to
    // the tool result rather than to stdout, and read at this server's
    // own injected clock.
    crate::json::to_json_string(&super::status::status_json(
        &run_id,
        &events,
        &manifest,
        ctx.clock.now(),
    ))
}

async fn tool_run_workflow(
    cwd: &Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, String> {
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
        .transpose()
        .map_err(|e| format!("invalid adapter: {e}"))?;
    let mode = args
        .get("mode")
        .and_then(Value::as_str)
        .map(str::parse::<ModeName>)
        .transpose()
        .map_err(|e| format!("invalid mode: {e}"))?;

    // Resolves `name` against the whole catalog — the repo's own
    // `.yunta/workflows/` then a publisher's vendored packs
    // (`acme/review`) — then creates the run and hands it off, all
    // in-process through the same `start_detached` `yunta run --detach`
    // calls.
    let ctx = Context::resolve_in(cwd.to_path_buf()).map_err(|e| e.to_string())?;
    let storage = ctx.async_storage().await.map_err(|e| e.to_string())?;
    let run_id = super::run::start_detached(
        &ctx,
        &storage,
        Path::new(name),
        &inputs,
        adapter.as_ref(),
        mode.as_ref(),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(format!("run_id: {run_id}"))
}

async fn tool_resume_run(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    let run_id = required_str(args, "run_id")?;
    let ctx = Context::resolve_in(cwd.to_path_buf()).map_err(|e| e.to_string())?;
    let run_dir = ctx.project.run_dir(run_id).ok_or_else(|| {
        format!(
            "no run `{run_id}` under {}",
            ctx.project.runs_root.display()
        )
    })?;
    super::spawn_detached_resume(&run_dir, run_id, cwd)
        .await
        .map_err(|e| format!("cannot spawn a detached `yunta resume {run_id}`: {e}"))?;
    Ok(format!(
        "run {run_id}: resumed, driving forward independently"
    ))
}

async fn tool_resolve_gate(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    let run_id = required_str(args, "run_id")?;
    let option = required_str(args, "option")?;
    let by = args
        .get("by")
        .and_then(Value::as_str)
        .map(str::parse::<yunta_core::Responder>)
        .transpose()
        .map_err(|e| e.to_string())?;
    let text = args.get("text").and_then(Value::as_str).map(str::to_string);

    let ctx = Context::resolve_in(cwd.to_path_buf()).map_err(|e| e.to_string())?;
    let run_id_typed = run_id
        .parse::<yunta_core::RunId>()
        .map_err(|e| e.to_string())?;
    let run_dir = ctx.project.run_dir(run_id).ok_or_else(|| {
        format!(
            "no run `{run_id}` under {}",
            ctx.project.runs_root.display()
        )
    })?;
    let manifest: yunta_core::Manifest = std::fs::read_to_string(run_dir.join("manifest.yaml"))
        .map_err(|e| e.to_string())
        .and_then(|text| yunta_core::yaml::parse(&text).map_err(|e| e.to_string()))?;
    let storage = ctx.async_storage().await.map_err(|e| e.to_string())?;

    yunta_engine::resolve_gate(
        &manifest,
        &storage,
        &run_id_typed,
        &yunta_core::SystemClock,
        yunta_core::events::HumanChoice {
            option: option
                .parse::<yunta_core::OptionId>()
                .map_err(|e| e.to_string())?,
            by: crate::identity::responder(by.as_ref()),
            free_text: text,
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    super::spawn_detached_resume(&run_dir, run_id, cwd)
        .await
        .map_err(|e| format!("decision recorded, but cannot spawn a detached resume: {e}"))?;
    Ok(format!(
        "run {run_id}: resolved `{option}`, driving forward independently"
    ))
}
