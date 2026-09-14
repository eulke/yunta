//! `artifacts.produces` — what a node declares it writes — and the two
//! ways anything else in a workflow names one of those artifacts.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use super::parse::{describe, list};
use crate::yaml::{Mapping, Value};

/// The run directory's own name for where artifacts live. Every path a
/// run records for an artifact is relative to the run directory and
/// starts here, which is the shape the log, the diagnostics and the run
/// contract all use — so writing one and reading one back read the same
/// name.
pub const ARTIFACTS_DIR: &str = "artifacts";

/// `artifacts.produces`. Each entry is one bare string: `tasks`,
/// `findings` and `questions` name the documents the engine reads, and
/// every other string is the name of a file it only carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Artifacts {
    pub produces: Vec<ArtifactSpec>,
}

/// One artifact named by a bare string: a node's `artifacts.produces`
/// entry, or one of the artifacts an external gate publishes.
///
/// A node produces at most one document of each kind, so a kind
/// identifies an interpreted artifact on its own and there is no file
/// name to choose. An opaque artifact has nothing else to go by, so its
/// name is what identifies it — which is why the three kind names are
/// not available as file names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum ArtifactSpec {
    /// A document the engine reads and validates, identified by its
    /// kind.
    Interpreted(ArtifactKind),
    /// A file the engine only carries, identified by its name.
    Opaque(String),
}

/// A string that names a kind *is* that kind; every other string is a
/// file name. One rule, read the same way wherever an artifact is named
/// by a bare string, so `tasks` never means a file in one place and a
/// document in another.
impl<'de> Deserialize<'de> for ArtifactSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        match Value::deserialize(deserializer)? {
            Value::String(text) => Ok(match text.parse::<ArtifactKind>() {
                Ok(kind) => ArtifactSpec::Interpreted(kind),
                Err(_) => ArtifactSpec::Opaque(text),
            }),
            other => Err(D::Error::custom(format!(
                "an artifact is a file name, or one of {} for a document the engine reads, \
                 not {}",
                ArtifactKind::listed(),
                describe(&other)
            ))),
        }
    }
}

impl ArtifactSpec {
    /// The kind the engine interprets it as, absent for an opaque
    /// artifact.
    pub fn kind(&self) -> Option<ArtifactKind> {
        match self {
            ArtifactSpec::Interpreted(kind) => Some(*kind),
            ArtifactSpec::Opaque(_) => None,
        }
    }
}

/// How an artifact names itself where a declaration is read back to a
/// person: by its kind, or by the file name it was declared under.
impl fmt::Display for ArtifactSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArtifactSpec::Interpreted(kind) => f.write_str(kind.as_str()),
            ArtifactSpec::Opaque(name) => f.write_str(name),
        }
    }
}

/// What a reference names an artifact by: `kind:` for a document the
/// engine reads, `name:` for a file it only carries.
///
/// The mapping form of [`ArtifactSpec`], for the references that carry
/// more than the artifact itself — a `context:` source and a `mounts:`
/// entry also name the node it comes from, and a mount also names what
/// the child carries it as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum ArtifactRefId {
    Kind { kind: ArtifactKind },
    Name { name: String },
}

impl ArtifactRefId {
    /// The keys a reference identifies its artifact with.
    pub(super) const KEYS: [&'static str; 2] = ["kind", "name"];

    /// Reads the `kind:` or `name:` of one reference out of what its
    /// container did not claim for itself.
    ///
    /// Exactly one of the two, because an artifact is a document of a
    /// kind or a file of a name and never both. `what` names the
    /// reference in the error and `claimed` the keys the container
    /// already took, so the listing an author reads is the whole
    /// reference's.
    pub(super) fn from_rest<'de, D: Deserializer<'de>>(
        rest: Mapping,
        what: &str,
        claimed: &[&str],
    ) -> Result<Self, D::Error> {
        use serde::de::Error;

        let valid: Vec<&str> = claimed.iter().copied().chain(ArtifactRefId::KEYS).collect();
        let unknown = |key: &dyn fmt::Display| {
            D::Error::custom(format!(
                "unknown key `{key}` for {what}; one of {}",
                list(&valid)
            ))
        };
        let mut id: Option<ArtifactRefId> = None;
        for (key, value) in rest {
            let Some(key) = key.as_str() else {
                return Err(unknown(&describe(&key)));
            };
            let read = match key {
                "kind" => ArtifactRefId::Kind {
                    kind: super::parse::nested::<D, _>(key, value)?,
                },
                "name" => ArtifactRefId::Name {
                    name: super::parse::nested::<D, _>(key, value)?,
                },
                other => return Err(unknown(&other)),
            };
            if id.replace(read).is_some() {
                return Err(D::Error::custom(format!(
                    "{what} names `kind:` or `name:`, never both: a document the engine reads \
                     is identified by its kind, and an artifact it only carries by its name"
                )));
            }
        }
        id.ok_or_else(|| {
            D::Error::custom(format!(
                "{what} names the artifact with `kind:` — one of {} — or with `name:`",
                ArtifactKind::listed()
            ))
        })
    }
}

/// How a reference names its artifact to a reader.
impl fmt::Display for ArtifactRefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArtifactRefId::Kind { kind } => f.write_str(kind.as_str()),
            ArtifactRefId::Name { name } => f.write_str(name),
        }
    }
}

/// A document the engine reads and validates, as opposed to an artifact
/// it only checks for existence. The kind fixes the document's shape, so
/// it is what every door — the workflow's own `kind:`, `yunta schema`,
/// the `document_shape` tool, a failed read's report — names it by.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    // Every door reads the kind through this derive, so an alias declared
    // here holds at all of them at once: a workflow, a frozen manifest or
    // an event log that spells the tasks document `task-ledger` reads as
    // `tasks`. A plain comment rather than rustdoc, so the published
    // schema names the kind by its one spelling.
    #[serde(alias = "task-ledger")]
    Tasks,
    Findings,
    Questions,
}

impl ArtifactKind {
    /// Every kind a door can be asked about, in the order a catalog
    /// lists them.
    pub const ALL: [ArtifactKind; 3] = [
        ArtifactKind::Tasks,
        ArtifactKind::Findings,
        ArtifactKind::Questions,
    ];

    /// How the kind names itself to a reader.
    pub fn label(self) -> &'static str {
        match self {
            ArtifactKind::Tasks => "tasks document",
            ArtifactKind::Findings => "findings artifact",
            ArtifactKind::Questions => "questions artifact",
        }
    }

    /// The value `kind:` carries in a workflow, and the argument
    /// `yunta schema` takes. Tied to what serde derives by a test, so
    /// the two spellings of these three names cannot drift apart. The
    /// canonical spelling only: a kind's alias is read, never written.
    pub fn as_str(self) -> &'static str {
        match self {
            ArtifactKind::Tasks => "tasks",
            ArtifactKind::Findings => "findings",
            ArtifactKind::Questions => "questions",
        }
    }

    /// The run tool a session submits a whole document of this kind
    /// through, when the kind is one a session submits whole.
    ///
    /// `None` for a kind whose entries accumulate one at a time: a
    /// findings artifact is the projection of what its node posted
    /// through [`POST_FINDING_TOOL`](ArtifactKind::POST_FINDING_TOOL),
    /// never a document handed over in one piece. One place, so the
    /// listing that mounts a tool, the dispatch that answers it and the
    /// sentence that names it to a session cannot disagree.
    pub fn submit_tool(self) -> Option<&'static str> {
        match self {
            ArtifactKind::Tasks => Some("yunta_submit_tasks"),
            ArtifactKind::Questions => Some("yunta_submit_questions"),
            ArtifactKind::Findings => None,
        }
    }

    /// The kind a submission tool name belongs to, or `None` for a name
    /// no kind submits through.
    pub fn from_submit_tool(name: &str) -> Option<Self> {
        ArtifactKind::ALL
            .into_iter()
            .find(|kind| kind.submit_tool() == Some(name))
    }

    /// The run tools one finding is posted, replaced and taken back
    /// through. Findings are the one kind whose entries arrive
    /// separately, so these are named rather than derived per kind.
    pub const POST_FINDING_TOOL: &'static str = "yunta_post_finding";
    pub const UPDATE_FINDING_TOOL: &'static str = "yunta_update_finding";
    pub const WITHDRAW_FINDING_TOOL: &'static str = "yunta_withdraw_finding";

    /// The kinds as a sentence lists them, so every door that has to
    /// say "one of ..." says it the same way.
    pub fn listed() -> String {
        ArtifactKind::ALL
            .iter()
            .map(|kind| format!("`{kind}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Names a kind that does not exist and lists the ones that do.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "`{value}` is not a document Yunta reads; one of {}",
    ArtifactKind::listed()
)]
pub struct UnknownArtifactKind {
    pub value: String,
}

/// Reads a kind the way every other door does — through `Deserialize`,
/// so the spellings `yunta schema` and the `document_shape` tool accept
/// are exactly the ones a workflow or an event log accepts, alias
/// included, with the derive as the one place that lists them.
impl std::str::FromStr for ArtifactKind {
    type Err = UnknownArtifactKind;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        use serde::de::IntoDeserializer;

        let deserializer: serde::de::value::StrDeserializer<'_, serde::de::value::Error> =
            value.into_deserializer();
        ArtifactKind::deserialize(deserializer).map_err(|_| UnknownArtifactKind {
            value: value.to_string(),
        })
    }
}

/// A name under a node's artifact view that the engine writes itself,
/// so an opaque artifact may not claim it.
///
/// The run's view of a node holds one file per identity. An interpreted
/// artifact is written from the document the engine accepted, and the
/// answers to a questions document are written beside it. A declared
/// name equal to one of those would put two writers on one path, and
/// the file a reader opened would say nothing about which of them wrote
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservedIdentity {
    /// The view of the document of this kind: `<kind>.yaml`.
    Kind(ArtifactKind),
    /// The answers the engine records beside a questions document.
    Answers,
}

impl ReservedIdentity {
    /// Every identity the engine writes, in the order a catalog lists
    /// them.
    pub const ALL: [ReservedIdentity; 4] = [
        ReservedIdentity::Kind(ArtifactKind::Tasks),
        ReservedIdentity::Kind(ArtifactKind::Findings),
        ReservedIdentity::Kind(ArtifactKind::Questions),
        ReservedIdentity::Answers,
    ];

    /// The file name this identity takes under a node's view — the one
    /// place that spelling is written down.
    pub fn file_name(&self) -> String {
        match self {
            ReservedIdentity::Kind(kind) => format!("{kind}.yaml"),
            ReservedIdentity::Answers => format!("{}.answers.yaml", ArtifactKind::Questions),
        }
    }

    /// The identity `name` claims, when it claims one.
    pub fn of(name: &str) -> Option<Self> {
        ReservedIdentity::ALL
            .into_iter()
            .find(|identity| identity.file_name() == name)
    }
}

impl fmt::Display for ReservedIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReservedIdentity::Kind(kind) => write!(f, "the `{kind}` document"),
            ReservedIdentity::Answers => write!(f, "the answers to a `questions` document"),
        }
    }
}

/// The name of an opaque artifact: where its file lands under the
/// node's own directory in the run's view.
///
/// Parsed, never assembled. A name reaches the view as a path joined to
/// the run directory, so one that is absolute or climbs with `..`
/// writes outside the run the moment it is used, and one that spells a
/// [`ReservedIdentity`] answers for a document the engine wrote. A name
/// can carry a template, and what a template renders to is a name like
/// any other: it is parsed again once it is known.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactName(String);

impl ArtifactName {
    /// The name `text` spells, or why it is not one.
    pub fn parse(text: &str) -> Result<Self, crate::diagnostic::Problem> {
        if text.is_empty() {
            return Err(crate::diagnostic::Problem::parse(
                "",
                "an artifact is named by a file name, and this one is empty",
            ));
        }
        if !crate::stays_inside(text) {
            return Err(crate::diagnostic::Problem::parse(
                "",
                format!(
                    "`{text}` reaches outside the run directory — an artifact name is relative, \
                     with no `..` component"
                ),
            ));
        }
        if let Some(identity) = ReservedIdentity::of(text) {
            return Err(crate::diagnostic::Problem::parse(
                "",
                format!("`{text}` is how the run's view names {identity}, so an artifact cannot take it"),
            ));
        }
        Ok(ArtifactName(text.to_string()))
    }

    /// The name, as the view spells it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArtifactName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
