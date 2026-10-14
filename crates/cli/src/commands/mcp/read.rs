//! The tools a client reads to learn how to act: the shape of a document Yunta
//! reads, and the catalog of workflows this repository offers.

use rmcp::model::Tool;
use serde_json::json;

use super::{document_kinds, empty_schema};

/// The definitions of `document_shape` and `list_workflows`.
pub(super) fn tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "document_shape",
            "The exact shape of a document Yunta reads and validates. Call it before you \
             write a tasks document, a spec, a findings artifact or a questions artifact: \
             they are validated strictly, a key that is not in the shape fails the node that \
             produced it, and this is the one place the format is written down. Returns a \
             complete, valid example with every field annotated.",
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
            "Lists this repository's own catalog of workflows (name, description, declared \
             inputs). Call it before you write ad-hoc code for a task a workflow may already \
             cover: a workflow's criteria and scope checks give a mechanical guarantee \
             free-form edits never do.",
            empty_schema(),
        ),
    ]
}
