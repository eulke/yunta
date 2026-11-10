//! What `yunta schema` shows a person: every kind of document Yunta reads
//! and what each is for, or every key a workflow reads, each with the
//! command that shows a shape and its JSON Schema.

use surco::components::{Fields, Next};
use surco::doc::{Block, Doc};
use surco::look::Look;
use surco::surface::Surface;
use surco::text::listed;
use yunta_core::step::{Arg, NextStep};
use yunta_core::{DocumentKind, NodeKind};

use super::Page;
use crate::command;

/// What a document's JSON Schema is for, wherever a page offers it.
const JSON_SCHEMA: &str = "the JSON Schema an editor validates against";

/// What `schema` shows a person: the documents it can show the shape of, or
/// every key a workflow reads.
pub enum Schema {
    /// Every kind of document Yunta reads, with what each is for.
    Documents,
    /// Every key a workflow reads.
    WorkflowKeys,
}

impl Schema {
    /// The shape a writer reads, as a control-plane client is handed it: a
    /// kind's annotated example, or the keys a workflow reads, as Markdown.
    pub fn shape(kind: DocumentKind) -> String {
        match kind {
            DocumentKind::Artifact(kind) => yunta_core::shape::contract(kind),
            DocumentKind::Workflow => {
                let markdown = surco::surface::Markdown;
                markdown.draw(&Schema::WorkflowKeys.document(&markdown.look()))
            }
        }
    }
}

impl Page for Schema {
    fn document(&self, _look: &Look) -> Doc {
        match self {
            Schema::Documents => list(),
            Schema::WorkflowKeys => workflow_keys(),
        }
    }
}

/// Every key a workflow reads: at its top and in every node, then in each
/// kind of node under a heading of its own, a row each; then the JSON
/// Schema an editor validates against.
fn workflow_keys() -> Doc {
    use yunta_core::workflow::{NODE_KEYS, WORKFLOW_KEYS};
    let document = Fields::new()
        .push_if("top level", listed(WORKFLOW_KEYS.iter().copied()))
        .push_if(
            "every node",
            listed(NODE_KEYS.iter().copied().chain([NodeKind::TAG])),
        );
    let kinds = NodeKind::KINDS.iter().fold(Fields::new(), |fields, kind| {
        fields.push_if(
            kind,
            listed(NodeKind::keys(kind).unwrap_or_default().iter().copied()),
        )
    });
    Doc::new()
        .with(Block::Heading("the keys a workflow reads".to_string()))
        .with(document)
        .with(Block::Heading(
            "the keys each kind of node reads".to_string(),
        ))
        .with(kinds)
        .with(Next {
            steps: vec![(
                command::line(&NextStep::Schema {
                    document: Arg::Given(DocumentKind::Workflow.as_str().to_string()),
                    json: true,
                }),
                JSON_SCHEMA,
            )],
        })
}

/// The catalog, one document per row with what it is for, and the two
/// commands that show a document's shape.
fn list() -> Doc {
    let documents = DocumentKind::all().fold(Fields::new(), |fields, shown| {
        fields.push_if(shown.as_str(), shown.purpose())
    });
    let next = Next {
        steps: vec![
            (
                command::line(&NextStep::Schema {
                    document: Arg::Blank,
                    json: false,
                }),
                "the shape to write",
            ),
            (
                command::line(&NextStep::Schema {
                    document: Arg::Blank,
                    json: true,
                }),
                JSON_SCHEMA,
            ),
        ],
    };
    Doc::new()
        .with(Block::Heading(
            "documents Yunta reads and validates".to_string(),
        ))
        .with(documents)
        .with(next)
}
