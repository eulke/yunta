//! `artifacts.produces` — what a node declares it writes.

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    TaskLedger,
    Findings,
    Questions,
}
