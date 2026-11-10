//! The control plane an agent asks Yunta through, as an MCP server. Seven
//! tools (`document_shape`, `list_workflows`, `run_workflow`,
//! `workflow_status`, `resume_run`, `resolve_gate`, `answer_questions`), none
//! of which ever blocks for a run's own duration: `run_workflow` creates the
//! run and hands it to a detached `yunta resume`, the other three that move
//! a run hand it over the same way, and each returns immediately; a caller
//! tracks progress by polling `workflow_status` (pull, never push, the same
//! model external gates use). A run never depends on this server: it goes on
//! when the client closes or the server is stopped, and the server is no
//! daemon.
//!
//! Each tool answers through `yunta-api`, as the twin of the command a
//! person runs: same project, same API call, same refusal, never by running
//! that command. `list_workflows` draws the catalog `yunta list` draws,
//! `workflow_status` serializes the document `yunta status --json` prints,
//! and `resolve_gate` and `answer_questions` are the second surface of a
//! door a run already answers through, never a second implementation of it.
//! `document_shape` returns the contract every other door publishes for a
//! document, the one place a client learns a format before writing it.
//!
//! [`ControlPlane`] has no transport of its own: an interface serves it on
//! the one it has, `yunta mcp` on stdio and a remote server on its own,
//! handing it the workspace it read, the stop it stops it with and the sink
//! that says what an operation remarks while it works.
//!
//! Its map: `tools`, what each tool does through the API; `read`, `runs`
//! and `answers`, each tool's definition, written to help a client
//! *decide*, not merely to describe; `error`, what a tool that did not do
//! what it was asked tells the agent.

mod answers;
mod error;
mod read;
mod runs;
mod tools;

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{json, Value};
use yunta_api::{Remarks, Stop, Workspace};
use yunta_core::DocumentKind;

use error::ToolError;

/// The control plane over a workspace: the tools an agent asks Yunta
/// through, answered with the API.
///
/// Not the MCP server a run's session asks its own run through, which the
/// engine serves for each run.
#[derive(Clone)]
pub struct ControlPlane {
    /// The workspace every tool works in, read once by the interface that
    /// serves the control plane.
    workspace: Arc<Workspace>,
    /// What stops the work a tool started.
    stop: Stop,
    /// Where what the API says while a tool works is said.
    remarks: Arc<dyn Remarks + Send>,
}

impl ControlPlane {
    /// The control plane over `workspace`, its work stopped by `stop`, what
    /// the API says while a tool works said through `remarks`: what an
    /// interface serves on its transport.
    pub fn new(workspace: Workspace, stop: Stop, remarks: Arc<dyn Remarks + Send>) -> Self {
        ControlPlane {
            workspace: Arc::new(workspace),
            stop,
            remarks,
        }
    }
}

impl ServerHandler for ControlPlane {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(yunta_mcp_protocol::tool_list(tool_definitions()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        let args = request.arguments.unwrap_or_default();
        let outcome = match request.name.as_ref() {
            "list_workflows" => tools::list_workflows(self).await,
            "workflow_status" => tools::workflow_status(self, &args).await,
            "run_workflow" => tools::run_workflow(self, &args).await,
            "resume_run" => tools::resume_run(self, &args).await,
            "resolve_gate" => tools::resolve_gate(self, &args).await,
            "answer_questions" => tools::answer_questions(self, &args).await,
            "document_shape" => document_shape(&args),
            // An unknown tool is a protocol error, not a tool that ran
            // and failed: the client asked for something this server
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
/// A tool that ran and failed answers with the sentence the equivalent
/// command prints on stderr, so an agent client asking this server gets the
/// advice a person at a terminal gets, rather than a second phrasing of the
/// same refusal.
fn tool_result(outcome: Result<String, ToolError>) -> rmcp::model::CallToolResponse {
    match outcome {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]).into(),
        Err(refusal) => CallToolResult::error(vec![ContentBlock::text(refusal.said())]).into(),
    }
}

fn empty_schema() -> serde_json::Map<String, Value> {
    json!({"type": "object", "properties": {}})
        .as_object()
        .cloned()
        .unwrap_or_default()
}

/// The shapes a client can ask for, as the tool's own enum, so the catalog
/// is visible the moment a client connects, before it calls anything.
fn document_kinds() -> Vec<&'static str> {
    DocumentKind::all().map(DocumentKind::as_str).collect()
}

/// The shape of one document, verbatim from the constant every other door
/// publishes.
///
/// The name is read as `yunta schema` reads it, so the two doors answer one
/// mistake with one sentence.
fn document_shape(args: &serde_json::Map<String, Value>) -> Result<String, ToolError> {
    let Some(name) = args.get("kind").and_then(Value::as_str) else {
        return Err(ToolError::Argument(format!(
            "`kind` is required: one of {}",
            surco::text::listed(document_kinds())
        )));
    };
    let kind = name.parse::<DocumentKind>().map_err(ToolError::unknown)?;
    Ok(yunta_render::page::Schema::shape(kind))
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
