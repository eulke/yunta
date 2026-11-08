//! `yunta schema`: the shape of a document Yunta reads and validates,
//! for whoever has to write one.
//!
//! This is the door for a shell — an agent working in a repo without the
//! control plane mounted, or a person writing the file by hand. It is
//! the only way either of them learns the format: `crates/core/schemas/`
//! is a development artifact of this repository, and whoever installed
//! the binary has no repository to read it from.
//!
//! The text is the same one a node's session receives and the
//! `document_shape` tool returns, from the same constant. Four doors
//! rendering four texts drift apart at the first schema change; four
//! doors rendering one do not.

use surco::components::{Fields, Next};
use surco::doc::{Block, Doc};
use surco::look::Look;
use surco::surface::Surface;
use surco::text::listed;
use yunta_core::shape::contract;
use yunta_core::step::{Arg, NextStep};
use yunta_core::{ArtifactKind, NodeKind, UnknownDocument};
use yunta_render::command;

use crate::error::CliError;

use crate::output::Outcome;
use crate::output::{raw, show, Mode};
use yunta_render::page::Page;

/// What a document's JSON Schema is for, wherever a page offers it.
const JSON_SCHEMA: &str = "the JSON Schema an editor validates against";

/// Prints one document's shape, or lists the documents when none is named.
pub fn schema(name: Option<&str>, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    let Some(name) = name else {
        if output == Mode::Json {
            return Err(CliError::msg(format!(
                "`--json` emits one document's schema: name a document, or run `{}` with no \
                 arguments to list them",
                command::line(&NextStep::Schemas)
            )));
        }
        show(look, &Schema::Documents);
        return Ok(Outcome::Success);
    };
    // One parse and one sentence: a name no document has is answered with
    // every name there is, and the one it is a slip from.
    match (name.parse::<Shown>()?, output) {
        (Shown::Workflow, Mode::Json) => raw(yunta_core::schema::workflow_json()),
        (Shown::Workflow, _) => show(look, &Schema::WorkflowKeys),
        // The committed bytes CI proves still match the types (`cargo
        // xtask schema --check`), never a schema generated here:
        // generating it would pull `schemars` and the whole
        // schema-building machinery into the shipped binary — around
        // 100 KB, reachable from this one flag — against a recorded
        // binary-size ceiling.
        (Shown::Artifact(kind), Mode::Json) => raw(yunta_core::schema::json(kind)),
        (Shown::Artifact(kind), _) => raw(&contract(kind)),
    }
    Ok(Outcome::Success)
}

/// A document `yunta schema` shows: what an artifact of a kind carries, or
/// a workflow file. The catalog lists these and the door reads them, and
/// no other name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shown {
    Artifact(ArtifactKind),
    Workflow,
}

impl Shown {
    /// Every document shown, in the order the catalog lists them.
    pub(crate) fn all() -> impl Iterator<Item = Shown> {
        ArtifactKind::ALL
            .into_iter()
            .map(Shown::Artifact)
            .chain([Shown::Workflow])
    }

    /// The name the door reads it by.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Shown::Artifact(kind) => kind.as_str(),
            Shown::Workflow => "workflow",
        }
    }

    /// The shape a writer reads, as a control-plane client is handed it:
    /// a kind's annotated example, or the keys a workflow reads.
    pub(crate) fn shape(self) -> String {
        match self {
            Shown::Artifact(kind) => contract(kind),
            Shown::Workflow => surco::surface::Markdown.draw(&workflow_keys()),
        }
    }

    /// What it is for, as the catalog says it.
    fn purpose(self) -> &'static str {
        match self {
            Shown::Artifact(kind) => kind.purpose(),
            Shown::Workflow => "the nodes a run executes, and how they connect",
        }
    }
}

impl std::str::FromStr for Shown {
    type Err = UnknownDocument;

    /// A document by its name, a kind's alias included; any other name is
    /// refused with every name there is.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        if name == Shown::Workflow.as_str() {
            return Ok(Shown::Workflow);
        }
        name.parse()
            .map(Shown::Artifact)
            .map_err(|_| UnknownDocument {
                value: name.to_string(),
                names: Shown::all().map(Shown::as_str).collect(),
            })
    }
}

/// What `schema` shows a person: the documents it can show the shape of, or
/// every key a workflow reads.
enum Schema {
    Documents,
    WorkflowKeys,
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
                    document: Arg::Given(Shown::Workflow.as_str().to_string()),
                    json: true,
                }),
                JSON_SCHEMA,
            )],
        })
}

/// The catalog, one document per row with what it is for, and the two
/// commands that show a document's shape.
fn list() -> Doc {
    let documents = Shown::all().fold(Fields::new(), |fields, shown| {
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
