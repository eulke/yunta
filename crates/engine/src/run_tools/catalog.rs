//! What a session is offered, and the schemas it is offered under.
//!
//! Every tool is built here so the list and its arguments stay one
//! decision: which tools a session sees depends on that session (a
//! blackboard group, a task, the kinds of document its node
//! declares), and each one's `inputSchema` comes from the schema this
//! repository publishes for the document it carries rather than from a
//! second description written by hand. A tool whose schema drifts from
//! the document the close validates is a promise the engine breaks, so
//! there is one source for both.

use rmcp::model::Tool;
use serde_json::{json, Value};
use yunta_core::ArtifactKind;

use super::session::SessionTools;

/// Every tool a session can be served.
///
/// The one place a run tool's name is written: the catalog builds from
/// it, the dispatch reads a call back through it, and a sentence that
/// tells a session to call one asks it for the name. A name spelled
/// twice is a tool a session is offered and the engine cannot answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTool {
    CheckArtifact,
    TaskStatus,
    GetBlackboard,
    RequestScopeExpansion,
    PostFinding,
    UpdateFinding,
    WithdrawFinding,
    /// One per kind a session submits a whole document of.
    Submit(ArtifactKind),
}

impl RunTool {
    /// Every tool, in the order a session reads them. The submission
    /// tools follow the kinds that have one.
    pub fn all() -> Vec<RunTool> {
        let mut all = vec![
            RunTool::CheckArtifact,
            RunTool::PostFinding,
            RunTool::UpdateFinding,
            RunTool::WithdrawFinding,
        ];
        all.extend(
            ArtifactKind::ALL
                .into_iter()
                .filter(|kind| kind.submit_tool().is_some())
                .map(RunTool::Submit),
        );
        all.extend([
            RunTool::TaskStatus,
            RunTool::RequestScopeExpansion,
            RunTool::GetBlackboard,
        ]);
        all
    }

    /// The name a session calls it by.
    pub fn name(self) -> &'static str {
        match self {
            RunTool::CheckArtifact => "yunta_check_artifact",
            RunTool::TaskStatus => "yunta_task_status",
            RunTool::GetBlackboard => "yunta_get_blackboard",
            RunTool::RequestScopeExpansion => "yunta_request_scope_expansion",
            RunTool::PostFinding => ArtifactKind::POST_FINDING_TOOL,
            RunTool::UpdateFinding => ArtifactKind::UPDATE_FINDING_TOOL,
            RunTool::WithdrawFinding => ArtifactKind::WITHDRAW_FINDING_TOOL,
            // A kind with no tool to submit through is never a
            // `Submit`: `all` builds them from the kinds that have one,
            // and `parse` reads a name back through the same door.
            RunTool::Submit(kind) => kind.submit_tool().unwrap_or_default(),
        }
    }

    /// The tool `name` is, or `None` for a name no tool answers to.
    pub fn parse(name: &str) -> Option<RunTool> {
        RunTool::all().into_iter().find(|tool| tool.name() == name)
    }

    /// Whether `session` is served this tool.
    fn offered_to(self, session: &SessionTools) -> bool {
        match self {
            RunTool::RequestScopeExpansion => session.task.is_some(),
            RunTool::GetBlackboard => session.in_blackboard_group(),
            RunTool::Submit(kind) => session.submits(kind),
            _ => true,
        }
    }

    /// The tool as the session is offered it: its name, what it does,
    /// and the shape of what it takes.
    fn declared(self) -> Tool {
        match self {
            RunTool::CheckArtifact => check_artifact_tool(),
            RunTool::TaskStatus => task_status_tool(),
            RunTool::GetBlackboard => blackboard_tool(),
            RunTool::RequestScopeExpansion => scope_expansion_tool(),
            RunTool::PostFinding => post_finding_tool(),
            RunTool::UpdateFinding => update_finding_tool(),
            RunTool::WithdrawFinding => withdraw_finding_tool(),
            RunTool::Submit(kind) => submit_tool(self.name(), kind),
        }
    }
}

/// The tools this session is served, in the order it reads them.
pub(super) fn mounted(session: &SessionTools) -> Vec<Tool> {
    RunTool::all()
        .into_iter()
        .filter(|tool| tool.offered_to(session))
        .map(RunTool::declared)
        .collect()
}

fn check_artifact_tool() -> Tool {
    Tool::new(
        "yunta_check_artifact",
        "Check an artifact this node declares, before your session ends: for a \
         file you wrote, that it is there and within its size; for a document you \
         submitted, what the engine read out of the file it wrote. Runs exactly the \
         verification the node's close runs, so a clean answer here is a clean \
         close. Omit `name` to check every artifact this node declares.",
        object(json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The artifact, as the node declares it: a kind for a \
                                    document the engine reads, a file name otherwise. \
                                    Omit to check them all."
                }
            }
        })),
    )
}

fn post_finding_tool() -> Tool {
    finding_tool(
        ArtifactKind::POST_FINDING_TOOL,
        "Report a structured finding the moment you see it — one call per \
         finding. The engine validates it at once: unknown keys, wrong types, \
         empty fields, and an id this node already used are refused with what \
         to fix, and nothing else you reported is lost. An accepted finding is \
         counted, deduplicated and consulted with every other finding on the \
         run, survives this session, and — when this node declares a `findings` \
         artifact — is written into that file at the end. Never write a findings \
         file yourself. To change a finding you reported, use \
         `yunta_update_finding`; to take one back, `yunta_withdraw_finding`.",
    )
}

fn update_finding_tool() -> Tool {
    finding_tool(
        ArtifactKind::UPDATE_FINDING_TOOL,
        "Replace a finding this node already reported, by id, with its whole \
         new content — same fields as `yunta_post_finding`, validated the same \
         way. Use it when a finding turns out to be more or less severe, wrongly \
         located, or better explained. Only a finding this node reported can be \
         updated, and a withdrawn one cannot; the previous state stays in the \
         run's log.",
    )
}

fn withdraw_finding_tool() -> Tool {
    Tool::new(
        ArtifactKind::WITHDRAW_FINDING_TOOL,
        "Take back a finding this node reported, by id, saying why — a non-empty \
         `reason`, such as a false positive or something fixed by other work. A \
         withdrawn finding is left out of every count, file and view from now on, \
         but the log keeps it and the reason. Withdrawal is final: a finding that \
         comes back is a new id.",
        withdrawal_schema(),
    )
}

fn task_status_tool() -> Tool {
    Tool::new(
        "yunta_task_status",
        "Read-only view of the run's tasks document (task id -> status) — the same data \
         the `tasks` context source mounts, queryable mid-session.",
        no_arguments(),
    )
}

fn scope_expansion_tool() -> Tool {
    Tool::new(
        "yunta_request_scope_expansion",
        "Ask the engine to widen this task's scope — you never widen it \
         yourself. Provide the paths, the reason, and a verifiable criterion that \
         is red today; the request is evaluated when this attempt ends, and a \
         denial becomes a finding rather than silence.",
        object(json!({
            "type": "object",
            "properties": {
                "paths": {"type": "array", "items": {"type": "string"}},
                "reason": {"type": "string"},
                "proposed_criterion": {"type": "object", "properties": {"cmd": {"type": "string"}}, "required": ["cmd"]},
            },
            "required": ["paths", "reason"],
        })),
    )
}

fn blackboard_tool() -> Tool {
    Tool::new(
        "yunta_get_blackboard",
        "Read this group's blackboard — your OWN posts only while the group runs \
         (siblings' posts become readable after the join, through the \
         group's consolidated output, so results never depend on arrival order).",
        no_arguments(),
    )
}

/// The tool a session submits a whole `kind` document through.
///
/// `document` is the kind's own published schema — the model fills in a
/// shape the engine already validates rather than transcribing a format
/// — and it is the only argument: the node declared the kind, so which
/// document this is was settled before the session started.
fn submit_tool(tool: &'static str, kind: ArtifactKind) -> Tool {
    let (document, defs) = published(kind);
    let properties = json!({ "document": document });
    Tool::new(
        tool,
        format!(
            "Submit this node's `{kind}` document as a structured object. The engine \
             validates it exactly as the node's close will — unknown keys, wrong types, \
             and the document's own rules — and writes the artifact file itself once it \
             is accepted. A refusal lists every problem to fix; submit again until it is \
             accepted. An acceptance reports what the engine read, so you can see your \
             meaning survived. Submitting again replaces the document."
        ),
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
fn finding_tool(name: &'static str, description: &'static str) -> Tool {
    let (properties, defs) = finding_entry_schema();
    Tool::new(
        name,
        description,
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
