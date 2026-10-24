//! What a session is offered, and the schemas it is offered under.
//!
//! Every tool is built here so the list and its arguments stay one
//! decision: which tools a session sees depends on that session (a
//! task, its node's own scope, a blackboard group, the kinds of document
//! its node declares), and each one's `inputSchema` comes from the schema this
//! repository publishes for the document it carries rather than from a
//! second description written by hand. A tool whose schema drifts from
//! the document the close validates is a promise the engine breaks, so
//! there is one source for both. The words a tool is described in are
//! written in [`words`].

use rmcp::model::Tool;
use serde_json::{json, Value};
use yunta_core::events::ExecutionEnvironment;
use yunta_core::ArtifactKind;
pub use yunta_core::RunTool;

use super::session::SessionTools;
use super::words;

trait RunToolCatalog {
    fn offered_to(self, session: &SessionTools) -> bool;
    fn declared(self, session: &SessionTools) -> Tool;
}

impl RunToolCatalog for RunTool {
    /// Whether `session` is served this tool.
    fn offered_to(self, session: &SessionTools) -> bool {
        match self {
            RunTool::Task | RunTool::CheckTask => session.task.is_some(),
            RunTool::CheckScope => session.node_scope.is_some(),
            RunTool::DeclareDeviation => session.task.is_some(),
            RunTool::RequestScopeExpansion => {
                session.task.is_some()
                    || session
                        .node_scope
                        .as_ref()
                        .is_some_and(|access| access.may_ask)
            }
            RunTool::GetBlackboard => session.in_blackboard_group(),
            RunTool::Submit(kind) => session.submits(kind),
            _ => true,
        }
    }

    /// The tool as the session is offered it: its name, what it does —
    /// naming any other tool the way this session's CLI does — and the
    /// shape of what it takes.
    fn declared(self, session: &SessionTools) -> Tool {
        match self {
            RunTool::CheckArtifact => check_artifact_tool(),
            RunTool::TaskStatus => task_status_tool(),
            RunTool::Task => task_tool(),
            RunTool::CheckTask => check_task_tool(),
            RunTool::GetBlackboard => blackboard_tool(),
            RunTool::CheckScope => check_scope_tool(session),
            RunTool::RequestScopeExpansion => scope_expansion_tool(),
            RunTool::DeclareDeviation => deviation_tool(),
            RunTool::PostFinding => post_finding_tool(session),
            RunTool::UpdateFinding => update_finding_tool(session),
            RunTool::WithdrawFinding => withdraw_finding_tool(),
            RunTool::Findings => findings_tool(session),
            RunTool::AnswerFinding => answer_finding_tool(session),
            RunTool::Submit(kind) => {
                submit_tool(self.name(), kind, session.host.environment.as_ref())
            }
        }
    }
}

/// The tools this session is served, in the order it reads them.
pub(super) fn mounted(session: &SessionTools) -> Vec<Tool> {
    offered(session)
        .into_iter()
        .map(|tool| tool.declared(session))
        .collect()
}

/// Which run tools this session is served, in the order it reads them.
pub(super) fn offered(session: &SessionTools) -> Vec<RunTool> {
    RunTool::all()
        .into_iter()
        .filter(|tool| tool.offered_to(session))
        .collect()
}

fn check_artifact_tool() -> Tool {
    Tool::new(
        RunTool::CheckArtifact.name(),
        words::check_artifact_description(),
        object(json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": words::artifact_name_description()
                }
            }
        })),
    )
}

fn post_finding_tool(session: &SessionTools) -> Tool {
    finding_tool(
        ArtifactKind::POST_FINDING_TOOL,
        &words::post_finding_description(
            &session.called(RunTool::UpdateFinding),
            &session.called(RunTool::WithdrawFinding),
        ),
    )
}

fn update_finding_tool(session: &SessionTools) -> Tool {
    finding_tool(
        ArtifactKind::UPDATE_FINDING_TOOL,
        &words::update_finding_description(&session.called(RunTool::PostFinding)),
    )
}

fn withdraw_finding_tool() -> Tool {
    Tool::new(
        ArtifactKind::WITHDRAW_FINDING_TOOL,
        words::withdraw_finding_description(),
        withdrawal_schema(),
    )
}

fn findings_tool(session: &SessionTools) -> Tool {
    Tool::new(
        RunTool::Findings.name(),
        words::findings_description(&session.called(RunTool::AnswerFinding)),
        no_arguments(),
    )
}

fn answer_finding_tool(session: &SessionTools) -> Tool {
    Tool::new(
        RunTool::AnswerFinding.name(),
        words::answer_finding_description(
            &session.called(RunTool::UpdateFinding),
            &session.called(RunTool::WithdrawFinding),
            &session.called(RunTool::Findings),
        ),
        object(json!({
            "type": "object",
            "properties": {
                "node": {"type": "string", "description": words::answer_node_description()},
                "id": {"type": "string", "description": words::answer_id_description()},
                "answer": {"type": "string", "enum": ["fixed", "declined"]},
                "why": {"type": "string"},
            },
            "required": ["node", "id", "answer", "why"],
            "additionalProperties": false,
        })),
    )
}

fn task_status_tool() -> Tool {
    Tool::new(
        RunTool::TaskStatus.name(),
        words::task_status_description(),
        no_arguments(),
    )
}

fn task_tool() -> Tool {
    Tool::new(
        RunTool::Task.name(),
        words::task_description(),
        no_arguments(),
    )
}

fn check_task_tool() -> Tool {
    Tool::new(
        RunTool::CheckTask.name(),
        words::check_task_description(),
        no_arguments(),
    )
}

fn check_scope_tool(session: &SessionTools) -> Tool {
    Tool::new(
        RunTool::CheckScope.name(),
        words::check_scope_description(&session.called(RunTool::RequestScopeExpansion)),
        no_arguments(),
    )
}

fn deviation_tool() -> Tool {
    Tool::new(
        RunTool::DeclareDeviation.name(),
        words::deviation_description(),
        object(json!({"type": "object", "oneOf": deviation_shapes()})),
    )
}

/// The two declarations the tool takes, told apart by `kind`: a departure
/// from the plan, and an unpassable criterion of the task.
fn deviation_shapes() -> Value {
    let named = |key: &str| {
        json!({"type": "object", "properties": {key: {"type": "string"}}, "required": [key],
               "additionalProperties": false})
    };
    json!([
        {
            "properties": {
                "kind": {"const": "departure"},
                "from": {"oneOf": [
                    named("shape"), named("decision"), named("change"), named("criterion"),
                    {"const": "outcome"},
                ]},
                "planned": {"type": "string"},
                "instead": {"type": "string"},
                "why": {"type": "string"},
            },
            "required": ["kind", "from", "planned", "instead", "why"],
            "additionalProperties": false,
        },
        {
            "properties": {
                "kind": {"const": "unpassable"},
                "criterion": {"type": "string"},
                "why": {"type": "string"},
            },
            "required": ["kind", "criterion", "why"],
            "additionalProperties": false,
        },
    ])
}

fn scope_expansion_tool() -> Tool {
    Tool::new(
        RunTool::RequestScopeExpansion.name(),
        words::scope_expansion_description(),
        object(json!({
            "type": "object",
            "properties": {
                "paths": {"type": "array", "items": {"type": "string"}},
                "reason": {"type": "string"},
                "proposed_criterion": {"type": "object", "properties": {"cmd": {"type": "string"}}, "required": ["cmd"]},
                "evidence": {"type": "string"},
            },
            "required": ["paths", "reason"],
        })),
    )
}

fn blackboard_tool() -> Tool {
    Tool::new(
        RunTool::GetBlackboard.name(),
        words::blackboard_description(),
        no_arguments(),
    )
}

/// The tool a session submits a whole `kind` document through.
///
/// `document` is the kind's own published schema — the model fills in a
/// shape the engine already validates rather than transcribing a format
/// — and it is the only argument: the node declared the kind, so which
/// document this is was settled before the session started.
///
/// A document whose commands the engine runs before accepting it says
/// where they run, so a command that names a program the engine cannot
/// find is not written in the first place.
fn submit_tool(
    tool: &'static str,
    kind: ArtifactKind,
    environment: Option<&ExecutionEnvironment>,
) -> Tool {
    let (document, defs) = published(kind);
    let properties = json!({ "document": document });
    let runs_under = match (kind, environment) {
        (ArtifactKind::Tasks | ArtifactKind::Spec, Some(environment)) => {
            words::runs_under_sentence(&environment.shell, &environment.path.join(":"))
        }
        _ => String::new(),
    };
    Tool::new(
        tool,
        words::submit_description(kind, &runs_under),
        tool_schema(
            properties.as_object().cloned().unwrap_or_default(),
            &["document"],
            defs,
        ),
    )
}

/// The fields a whole finding carries. Posting one and replacing one take
/// the same document, so they ask for the same thing.
const FINDING_FIELDS: [&str; 5] = ["id", "severity", "title", "location", "detail"];

/// A tool that takes one finding, under the entry schema the findings
/// document publishes.
fn finding_tool(name: &'static str, description: &str) -> Tool {
    let (properties, defs) = finding_entry_schema();
    Tool::new(
        name,
        description.to_string(),
        tool_schema(properties, &FINDING_FIELDS, defs),
    )
}

/// One tool's `inputSchema`, built from the schema the repository
/// publishes for a document rather than written out again here.
///
/// `properties` is what the tool takes; `$defs` is hoisted to the root so
/// every `#/$defs/...` reference the published schema carries still
/// resolves. Writing a schema twice is writing two schemas: this is the
/// one place a published document becomes a tool's arguments.
fn tool_schema(
    properties: serde_json::Map<String, Value>,
    required: &[&str],
    defs: Option<Value>,
) -> serde_json::Map<String, Value> {
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties,
    });
    let mut schema = schema.as_object().cloned().unwrap_or_default();
    if let Some(defs) = defs {
        schema.insert("$defs".to_string(), defs);
    }
    schema
}

/// The published schema of `kind`, split into the part that describes
/// the document and the definitions it refers to.
fn published(kind: ArtifactKind) -> (Value, Option<Value>) {
    let mut schema: Value = serde_json::from_str(yunta_core::schema::json(kind))
        .unwrap_or_else(|_| json!({"type": "object"}));
    let defs = schema
        .as_object_mut()
        .and_then(|object| object.remove("$defs"));
    if let Some(object) = schema.as_object_mut() {
        for key in ["$schema", "$id", "title", "description"] {
            object.remove(key);
        }
    }
    (schema, defs)
}

/// The schema of a withdrawal, as the repository publishes it.
fn withdrawal_schema() -> serde_json::Map<String, Value> {
    let mut schema: Value =
        serde_json::from_str(include_str!("../../../core/schemas/withdrawal.json"))
            .unwrap_or_else(|_| json!({"type": "object"}));
    let defs = schema
        .as_object_mut()
        .and_then(|object| object.remove("$defs"));
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    tool_schema(properties, &["id", "reason"], defs)
}

/// The definition of one finding, from the published findings schema:
/// the entry its `findings` list holds.
fn finding_entry_schema() -> (serde_json::Map<String, Value>, Option<Value>) {
    let (document, defs) = published(ArtifactKind::Findings);
    let entry = document
        .pointer("/properties/findings/items/$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| defs.as_ref().and_then(|defs| defs.get(name)).cloned())
        .unwrap_or_else(|| json!({"type": "object"}));
    let properties = entry
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    (properties, defs)
}

/// A schema literal as the object a tool carries.
fn object(schema: Value) -> serde_json::Map<String, Value> {
    schema.as_object().cloned().unwrap_or_default()
}

/// The schema of a tool that takes nothing.
fn no_arguments() -> serde_json::Map<String, Value> {
    object(json!({"type": "object", "properties": {}}))
}
