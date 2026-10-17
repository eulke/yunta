//! The tools that start a run and read where it stands: `run_workflow`
//! hands a run off, `workflow_status` reads it, and `resume_run` hands a
//! paused one back.

use rmcp::model::Tool;
use serde_json::json;

/// The definitions of `run_workflow`, `workflow_status` and `resume_run`.
pub(super) fn tools() -> Vec<Tool> {
    vec![
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
                    "inputs": {"type": "object", "description": "each declared input's name, with its value", "additionalProperties": {"type": "string"}},
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
             with nothing else for resolve_gate to answer. A run paused on an account's \
             limit waits for that limit's reset first, and the call still returns at once.",
            json!({
                "type": "object",
                "properties": {"run_id": {"type": "string"}},
                "required": ["run_id"],
            })
            .as_object().cloned().unwrap_or_default(),
        ),
    ]
}
