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

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Value};

use crate::error::{CliError, Outcome};
use crate::project;

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
            other => Err(format!("unknown tool `{other}`")),
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

fn tool_definitions() -> Vec<Tool> {
    vec![
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
             run_workflow/resume_run/resolve_gate call actually finished.",
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

/// Shells out to this same binary's own `list`/`status` rendering
/// rather than a second copy of it — `current_exe()` falling back to
/// the bare name matches
/// `spawn_detached_resume`'s own convention (PATH lookup is the worst
/// case, not a silent failure).
async fn run_self(cwd: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("yunta"));
    let output = tokio::process::Command::new(exe)
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| format!("failed to run `yunta {}`: {e}", args.join(" ")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        Err(format!("{stdout}{stderr}"))
    }
}

async fn tool_list_workflows(cwd: &std::path::Path) -> Result<String, String> {
    run_self(cwd, &["list"]).await
}

fn required_str<'a>(
    args: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or non-string argument `{name}`"))
}

async fn tool_workflow_status(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    let run_id = required_str(args, "run_id")?;
    run_self(cwd, &["status", run_id]).await
}

async fn tool_run_workflow(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    let name = required_str(args, "workflow")?;
    let path = cwd.join(".yunta/workflows").join(format!("{name}.yaml"));
    if !path.exists() {
        return Err(format!(
            "no workflow `{name}` in this repo's catalog (`{}`) — call list_workflows first",
            path.display()
        ));
    }
    let path_str = path.display().to_string();
    let mut owned_args: Vec<String> = vec!["run".to_string(), path_str, "--detach".to_string()];
    if let Some(inputs) = args.get("inputs").and_then(Value::as_object) {
        for (key, value) in inputs {
            let rendered = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            owned_args.push("--input".to_string());
            owned_args.push(format!("{key}={rendered}"));
        }
    }
    if let Some(adapter) = args.get("adapter").and_then(Value::as_str) {
        owned_args.push("--adapter".to_string());
        owned_args.push(adapter.to_string());
    }
    if let Some(mode) = args.get("mode").and_then(Value::as_str) {
        owned_args.push("--mode".to_string());
        owned_args.push(mode.to_string());
    }
    let arg_refs: Vec<&str> = owned_args.iter().map(String::as_str).collect();
    let stdout = run_self(cwd, &arg_refs).await?;
    stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("run ")
                .and_then(|rest| rest.split(':').next())
                .map(str::to_string)
        })
        .map(|run_id| format!("run_id: {run_id}"))
        .ok_or_else(|| format!("could not find a run id in `yunta run`'s own output: {stdout}"))
}

async fn tool_resume_run(
    cwd: &std::path::Path,
    args: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    let run_id = required_str(args, "run_id")?;
    let project = project::resolve(cwd).map_err(|e| e.to_string())?;
    let run_dir = project::find_run_dir(&project, run_id)
        .ok_or_else(|| format!("no run `{run_id}` under {}", project.runs_root.display()))?;
    super::spawn_detached_resume(&run_dir, run_id, cwd)
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
    let by = args.get("by").and_then(Value::as_str).map(str::to_string);
    let text = args.get("text").and_then(Value::as_str).map(str::to_string);

    let project = project::resolve(cwd).map_err(|e| e.to_string())?;
    let run_id_typed = run_id
        .parse::<yunta_core::RunId>()
        .map_err(|e| e.to_string())?;
    let run_dir = project::find_run_dir(&project, run_id)
        .ok_or_else(|| format!("no run `{run_id}` under {}", project.runs_root.display()))?;
    let manifest: yunta_core::Manifest = std::fs::read_to_string(run_dir.join("manifest.yaml"))
        .map_err(|e| e.to_string())
        .and_then(|text| yunta_core::yaml::parse(&text).map_err(|e| e.to_string()))?;
    let storage = yunta_storage::AsyncStorage::open(&project.storage_path)
        .await
        .map_err(|e| e.to_string())?;

    yunta_engine::resolve_gate(
        &manifest,
        &storage,
        &run_id_typed,
        &yunta_core::SystemClock,
        option,
        by,
        text,
    )
    .await
    .map_err(|e| e.to_string())?;

    super::spawn_detached_resume(&run_dir, run_id, cwd)
        .map_err(|e| format!("decision recorded, but cannot spawn a detached resume: {e}"))?;
    Ok(format!(
        "run {run_id}: resolved `{option}`, driving forward independently"
    ))
}
