//! The tools that answer a paused run: `resolve_gate` records the decision
//! its gate waits on, and `answer_questions` records the answers its node
//! asked for.

use rmcp::model::Tool;
use serde_json::json;

/// The definitions of `resolve_gate` and `answer_questions`.
pub(super) fn tools() -> Vec<Tool> {
    vec![resolve_gate(), answer_questions()]
}

fn resolve_gate() -> Tool {
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
                "by": {"type": "string", "description": "who answers, for the audit trail"},
                "text": {"type": "string", "description": "free-form context alongside the choice"},
            },
            "required": ["run_id", "option"],
        })
        .as_object()
        .cloned()
        .unwrap_or_default(),
    )
}

fn answer_questions() -> Tool {
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
                "by": {"type": "string", "description": "who answers, for the audit trail"},
            },
            "required": ["run_id", "node", "answers"],
        })
        .as_object().cloned().unwrap_or_default(),
    )
}
