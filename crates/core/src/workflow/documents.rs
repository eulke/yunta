//! The kinds of document Yunta reads and validates, and how a name is read
//! as one: a kind of artifact, or a workflow file. `yunta schema`, the
//! control plane's `document_shape` tool and a workflow's own keys read a
//! name the same way, so one mistake gets one sentence.

use serde::Deserialize;

use super::ArtifactKind;

/// A kind of document Yunta reads and validates: what an artifact of a kind
/// carries, or a workflow file. `yunta schema`, the control plane's
/// `document_shape` tool and the page that lists them read these, and no
/// other name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Artifact(ArtifactKind),
    Workflow,
}

impl DocumentKind {
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
