//! The kinds of document Yunta reads and validates — a workflow file, or a
//! kind of artifact a node produces — and how a name is read as one.
//! `yunta schema`, the control plane's `document_shape` tool, a workflow's
//! own keys and every report a document gets read the same kinds, so one
//! mistake gets one sentence.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ArtifactKind;

/// What kind of document a report is about.
///
/// Every document this system reads is held to a shape and to rules,
/// and reports what it found the same way — so a workflow that breaks
/// its own rules reaches a reader in the shape a tasks document already
/// does. The two are separate arms because they are read at different
/// moments by different doors: a workflow before any run, an artifact
/// at a node's close.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentKind {
    /// The workflow file a run is created from.
    Workflow,
    /// A document a node produces and the engine reads.
    #[serde(untagged)]
    Artifact(ArtifactKind),
}

impl DocumentKind {
    /// How the kind names itself to a reader.
    pub fn label(self) -> &'static str {
        match self {
            DocumentKind::Workflow => "workflow",
            DocumentKind::Artifact(kind) => kind.label(),
        }
    }

    /// Every document shown, in the order the catalog lists them.
    pub fn all() -> impl Iterator<Item = DocumentKind> {
        ArtifactKind::ALL
            .into_iter()
            .map(DocumentKind::Artifact)
            .chain([DocumentKind::Workflow])
    }

    /// The name the door reads it by.
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentKind::Artifact(kind) => kind.as_str(),
            DocumentKind::Workflow => "workflow",
        }
    }

    /// What it is for, as the catalog says it.
    pub fn purpose(self) -> &'static str {
        match self {
            DocumentKind::Artifact(kind) => kind.purpose(),
            DocumentKind::Workflow => "the nodes a run executes, and how they connect",
        }
    }
}

impl From<ArtifactKind> for DocumentKind {
    fn from(kind: ArtifactKind) -> Self {
        DocumentKind::Artifact(kind)
    }
}

impl fmt::Display for DocumentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocumentKind::Workflow => f.write_str("workflow"),
            DocumentKind::Artifact(kind) => fmt::Display::fmt(kind, f),
        }
    }
}

impl std::str::FromStr for DocumentKind {
    type Err = UnknownDocument;

    /// A document by its name, a kind's alias included; any other name is
    /// refused with every name there is.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        if name == DocumentKind::Workflow.as_str() {
            return Ok(DocumentKind::Workflow);
        }
        name.parse()
            .map(DocumentKind::Artifact)
            .map_err(|_| UnknownDocument {
                value: name.to_string(),
                names: DocumentKind::all().map(DocumentKind::as_str).collect(),
            })
    }
}

/// Names a document Yunta does not read, every name a door reads one by,
/// and the one it is most likely a slip from.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "`{value}` is not a document Yunta reads; one of {}{}",
    surco::text::listed(.names.iter().copied()),
    surco::text::near_miss(surco::text::nearest(.value, .names.iter().copied()))
)]
pub struct UnknownDocument {
    pub value: String,
    pub names: Vec<&'static str>,
}

/// Reads a kind the way every other door does — through `Deserialize`,
/// so the spellings `yunta schema` and the `document_shape` tool accept
/// are exactly the ones a workflow or an event log accepts, alias
/// included, with the derive as the one place that lists them.
impl std::str::FromStr for ArtifactKind {
    type Err = UnknownDocument;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        use serde::de::IntoDeserializer;

        let deserializer: serde::de::value::StrDeserializer<'_, serde::de::value::Error> =
            value.into_deserializer();
        ArtifactKind::deserialize(deserializer).map_err(|_| UnknownDocument {
            value: value.to_string(),
            names: ArtifactKind::ALL.iter().map(|kind| kind.as_str()).collect(),
        })
    }
}
