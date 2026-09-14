//! What a run holds: what a session handed over, what the engine
//! accepted into the store, and what a node wrote.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::hash::ContentHash;
use crate::ids::{NodeId, RunId};

// A kind nothing in this workspace writes. It is in the log's
// vocabulary because a log may hold one — written by something that is
// not this engine — and a reader that could not parse it would report a
// run's own history as unknown. So it has a fold and a rendering and no
// constructor: there is nothing here to build one with, which is what
// says the engine does not emit it. `Serialize` is the wire enum's
// requirement, not an emitter's. The comment is not a doc comment
// because a doc comment here would be published in `events.json`, where
// it would describe the log to a reader who is not writing Rust.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactWrittenPayload {
    pub path: PathBuf,
    pub content_hash: ContentHash,
    /// The artifact's declared `kind:` when it has one — what
    /// lets `derive()` recognize a `questions` artifact without reading
    /// any file (state comes from events alone). `None` for opaque
    /// artifacts and for logs written before the field existed (tolerant
    /// reader). Named `artifact_kind`, not `kind`: the event
    /// envelope's own internally-tagged discriminant already claims
    /// `kind` in the serialized JSON, and a colliding field name
    /// silently corrupts the payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_kind: Option<crate::workflow::ArtifactKind>,
}

/// What a session offered as a whole document, and what the engine
/// answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactSubmittedPayload {
    pub name: String,
    /// Named `artifact_kind`, not `kind`, for the reason
    /// [`ArtifactWrittenPayload`] gives: the envelope's own tag already
    /// claims `kind` in the serialized JSON.
    pub artifact_kind: crate::workflow::ArtifactKind,
    pub outcome: SubmissionOutcome,
}

/// Accepted, and the file the engine wrote from it; or refused, and
/// every problem the document has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionOutcome {
    Accepted {
        content_hash: crate::hash::ContentHash,
    },
    Refused {
        report: crate::diagnostic::Report,
    },
}

/// One artifact the run holds: what it is, the bytes it is made of, and
/// how the run came by it.
///
/// This is what makes an artifact a fact of the log rather than a file
/// somebody may have replaced: the identity says which artifact, the
/// hash says which content, and the envelope's `node_id` says who
/// produced it — absent for what a run acquires without a node of its
/// own (a `document` input, a mount, a promotion).
///
/// The field is named `artifact`, not `kind`, for the reason
/// [`ArtifactWrittenPayload::artifact_kind`] gives: the event
/// envelope's own internally-tagged discriminant already claims `kind`
/// in the serialized JSON, and a colliding field name silently corrupts
/// the payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactAcceptedPayload {
    pub artifact: ArtifactId,
    pub content_hash: ContentHash,
    pub origin: ArtifactOrigin,
}

impl ArtifactAcceptedPayload {
    /// The engine took `artifact` into the store: what it is, the bytes
    /// it now holds under that identity, and where they came from.
    ///
    /// All three together, always: an acceptance without its hash names
    /// no bytes, and one without its origin cannot say whether a person,
    /// a session or the engine itself produced them.
    pub fn new(artifact: ArtifactId, content_hash: ContentHash, origin: ArtifactOrigin) -> Self {
        ArtifactAcceptedPayload {
            artifact,
            content_hash,
            origin,
        }
    }
}

/// What an artifact is, which is what a reader asks for it by.
///
/// An artifact the engine interprets is identified by its kind: a run
/// holds one tasks document, whoever produced it, so a reader asks for
/// the kind and never needs to know the file name its workflow chose.
/// One the engine only carries is identified by the name the workflow
/// declared, which is the only thing that distinguishes it from any
/// other opaque artifact.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ArtifactId {
    Interpreted { kind: crate::workflow::ArtifactKind },
    Opaque { name: String },
}

impl ArtifactId {
    /// What an artifact written as `name` under `kind` is, for a log
    /// written before an acceptance stated the identity itself. A
    /// recorded `kind` *is* the identity; without one the artifact is
    /// opaque and the name it was written as is all that names it.
    pub fn of(name: &str, kind: Option<crate::workflow::ArtifactKind>) -> Self {
        match kind {
            Some(kind) => ArtifactId::Interpreted { kind },
            None => ArtifactId::Opaque {
                name: name.to_string(),
            },
        }
    }

    /// How an artifact names itself inside a sentence: an interpreted
    /// one by what it is, an opaque one by the name that is all there
    /// is to call it.
    pub fn label(&self) -> String {
        match self {
            ArtifactId::Interpreted { kind } => kind.label().to_string(),
            ArtifactId::Opaque { name } => format!("artifact `{name}`"),
        }
    }

    /// The file name the run's `artifacts/` view carries this artifact
    /// under.
    ///
    /// The one place a view is named, so the file a reader opens, the
    /// path a diagnostic prints and the copy a distillation writes are
    /// the same name. An interpreted document is named by its kind —
    /// a run holds one of each per producer, so there is nothing else to
    /// tell them apart — and an opaque artifact by the name that is all
    /// that identifies it.
    pub fn view_name(&self) -> String {
        match self {
            ArtifactId::Interpreted { kind } => format!("{kind}.yaml"),
            ArtifactId::Opaque { name } => name.clone(),
        }
    }
}

/// What a node declares it produces *is* an identity: the one place a
/// declaration becomes the question a log answers.
impl From<&crate::workflow::ArtifactSpec> for ArtifactId {
    fn from(spec: &crate::workflow::ArtifactSpec) -> Self {
        match spec {
            crate::workflow::ArtifactSpec::Interpreted(kind) => {
                ArtifactId::Interpreted { kind: *kind }
            }
            crate::workflow::ArtifactSpec::Opaque(name) => {
                ArtifactId::Opaque { name: name.clone() }
            }
        }
    }
}

/// And so is what a reference names: the mapping form of the same two
/// answers.
impl From<&crate::workflow::ArtifactRefId> for ArtifactId {
    fn from(id: &crate::workflow::ArtifactRefId) -> Self {
        match id {
            crate::workflow::ArtifactRefId::Kind { kind } => {
                ArtifactId::Interpreted { kind: *kind }
            }
            crate::workflow::ArtifactRefId::Name { name } => {
                ArtifactId::Opaque { name: name.clone() }
            }
        }
    }
}

/// How an artifact names itself to a reader: an interpreted one by its
/// kind, an opaque one by the name it was declared under.
impl std::fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactId::Interpreted { kind } => write!(f, "{kind}"),
            ArtifactId::Opaque { name } => write!(f, "{name}"),
        }
    }
}

/// How the run came by an artifact.
///
/// The origin is what tells apart an artifact a node produced from one
/// the run was handed, which no hash and no name can: two runs holding
/// the same tasks document differ in whether they planned it or
/// inherited it, and every rule about who may replace an artifact reads
/// that difference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactOrigin {
    /// A session handed the whole document over through its submission
    /// tool.
    Submitted,
    /// A command node wrote the file and the engine took it as declared.
    Ingested,
    /// The engine derived the document from the log itself, as it does
    /// for a findings artifact.
    Derived,
    /// The answers to a questions artifact.
    Answered,
    /// A `type: document` input, named by the input it came in as.
    Input { input: String },
    /// Another run's artifact: a mount, a child's output, a promotion.
    /// `producer` is the node that produced it there, absent when that
    /// run acquired it without a node either.
    Inherited {
        run: RunId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        producer: Option<NodeId>,
    },
    /// A log that recorded the artifact without saying where it came
    /// from — every `artifact_written` there is. The honest origin of a
    /// fact stated before origins were: the run held it, and the log
    /// does not say how.
    Legacy,
}
