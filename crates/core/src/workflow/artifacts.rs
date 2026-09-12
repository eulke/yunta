//! `artifacts.produces` — what a node declares it writes.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use super::parse::{describe, nested};
use crate::yaml::Value;

/// `artifacts.produces`. `task-ledger`, `findings` and
/// `questions` are interpreted. A plain string stays
/// opaque.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Artifacts {
    pub produces: Vec<ArtifactSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum ArtifactSpec {
    Plain(String),
    Typed { name: String, kind: ArtifactKind },
}

impl<'de> Deserialize<'de> for ArtifactSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        #[derive(Deserialize, schemars::JsonSchema)]
        #[serde(deny_unknown_fields)]
        struct Typed {
            name: String,
            kind: ArtifactKind,
        }

        match Value::deserialize(deserializer)? {
            Value::String(name) => Ok(ArtifactSpec::Plain(name)),
            mapping @ Value::Mapping(_) => {
                let Typed { name, kind } = nested::<D, _>("artifact", mapping)?;
                Ok(ArtifactSpec::Typed { name, kind })
            }
            other => Err(D::Error::custom(format!(
                "an artifact is a name or `{{ name: <name>, kind: <kind> }}`, not {}",
                describe(&other)
            ))),
        }
    }
}

impl ArtifactSpec {
    /// The file name under the run's `artifacts/`, whichever form the
    /// author used to declare it.
    pub fn name(&self) -> &str {
        match self {
            ArtifactSpec::Plain(name) => name,
            ArtifactSpec::Typed { name, .. } => name,
        }
    }

    /// The kind the engine interprets it as, absent for an opaque
    /// artifact.
    pub fn kind(&self) -> Option<ArtifactKind> {
        match self {
            ArtifactSpec::Plain(_) => None,
            ArtifactSpec::Typed { kind, .. } => Some(*kind),
        }
    }
}

/// A document the engine reads and validates, as opposed to an artifact
/// it only checks for existence. The kind fixes the document's shape, so
/// it is what every door — the workflow's own `kind:`, `yunta schema`,
/// the `document_shape` tool, a failed read's report — names it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    TaskLedger,
    Findings,
    Questions,
}

impl ArtifactKind {
    /// Every kind a door can be asked about, in the order a catalog
    /// lists them.
    pub const ALL: [ArtifactKind; 3] = [
        ArtifactKind::TaskLedger,
        ArtifactKind::Findings,
        ArtifactKind::Questions,
    ];

    /// How the kind names itself to a reader.
    pub fn label(self) -> &'static str {
        match self {
            ArtifactKind::TaskLedger => "task ledger",
            ArtifactKind::Findings => "findings artifact",
            ArtifactKind::Questions => "questions artifact",
        }
    }

    /// The value `kind:` carries in a workflow, and the argument
    /// `yunta schema` takes. Tied to what serde derives by a test, so
    /// the two spellings of these three names cannot drift apart.
    pub fn as_str(self) -> &'static str {
        match self {
            ArtifactKind::TaskLedger => "task-ledger",
            ArtifactKind::Findings => "findings",
            ArtifactKind::Questions => "questions",
        }
    }

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

impl std::str::FromStr for ArtifactKind {
    type Err = UnknownArtifactKind;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        ArtifactKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| UnknownArtifactKind {
                value: value.to_string(),
            })
    }
}
