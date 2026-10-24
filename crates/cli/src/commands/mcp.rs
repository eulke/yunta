//! `yunta mcp`: the control plane's MCP server, stdio. Seven tools —
//! `document_shape`, `list_workflows`, `run_workflow`,
//! `workflow_status`, `resume_run`, `resolve_gate`, `answer_questions`
//! — none of which ever blocks for a run's own duration:
//! `run_workflow` creates the run and hands it to a detached `yunta
//! resume`, the other three that move a run hand it over the same way,
//! and each returns immediately; a caller tracks progress by polling
//! `workflow_status` (pull, never push — the same model external gates
//! use). A run never depends on this server: it goes on when the client
//! closes or the server is killed, and the server is no daemon.
//!
//! Each tool answers in-process, as the twin of the command a person runs
//! — same `Context`, same engine call, same refusal — and never by running
//! that command, whose output would land on the stdout the JSON-RPC
//! replies travel on. `list_workflows` draws the catalog `yunta list`
//! draws, `workflow_status` serializes the document `yunta status --json`
//! prints, and `resolve_gate` and `answer_questions` are the second
//! surface of a door a run already answers through
//! (`commands::resolve_gate::resolve`, `yunta_engine::answer_questions`),
//! never a second implementation of it. `document_shape` returns the
//! contract every other door publishes for a document, the one place a
//! client learns a format before writing it.
//!
//! Tool descriptions are written to help a client *decide*, not merely
//! to describe — each names when reaching for a verified workflow
//! beats implementing directly.

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Value};
use yunta_core::ArtifactKind;

use crate::error::{CliError, Outcome};

mod answers;
mod read;
mod runs;
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
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
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
            Err(source) => return Ok(tool_result(Err(CliError::Cwd { source }))),
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
        Err(refusal) => CallToolResult::error(vec![ContentBlock::text(
            refusal.said_to(crate::error::Reader::Agent),
        )])
        .into(),
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

/// The advertised tools, in the order a client is shown them: the reads
/// first, then the calls that start and answer a run.
fn tool_definitions() -> Vec<Tool> {
    let mut tools = read::tools();
    tools.extend(runs::tools());
    tools.extend(answers::tools());
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every description a client reads: each tool's, and each of its
    /// parameters'.
    fn descriptions() -> Vec<String> {
        fn within(value: &Value, found: &mut Vec<String>) {
            match value {
                Value::Object(map) => {
                    if let Some(Value::String(text)) = map.get("description") {
                        found.push(text.clone());
                    }
                    map.values().for_each(|inner| within(inner, found));
                }
                Value::Array(items) => items.iter().for_each(|inner| within(inner, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for tool in tool_definitions() {
            found.extend(tool.description.as_deref().map(str::to_string));
            within(&Value::Object((*tool.input_schema).clone()), &mut found);
        }
        found
    }

    #[test]
    fn tool_descriptions_use_no_capitals_or_arrows() {
        let descriptions = descriptions();
        assert!(descriptions.len() > 7, "{descriptions:?}");
        for text in descriptions {
            assert!(!text.contains("->"), "an arrow where words go: {text}");
            assert!(!text.contains("'s answering"), "a contraction: {text}");
            let shouted = text
                .split(|c: char| !c.is_alphanumeric())
                .find(|word| word.len() > 4 && word.chars().all(|c| c.is_ascii_uppercase()));
            assert_eq!(shouted, None, "a word in capitals: {text}");
        }
    }
}
