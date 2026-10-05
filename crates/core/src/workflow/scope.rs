//! `scope:` on a node — what its work is answerable for: nothing
//! declared, a list of globs, or `run`, the paths the run has changed
//! since its base.
//!
//! `run` is how a node that corrects the run's own work — a fix for what
//! a lint reported — declares its reach without naming a path of a
//! repository it has never seen. The paths are the run's; the engine
//! resolves them when the node starts and records them on the log.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::parse::describe;
use crate::glob::ScopeGlob;
use crate::yaml::Value;

/// What a node declares its work answerable for.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum NodeScope {
    /// Nothing declared: the node works in the run's own tree and is not
    /// audited.
    #[default]
    Unscoped,
    /// The globs the node may change, never empty.
    Globs(Vec<ScopeGlob>),
    /// The paths the run changed since its base, as they stand when the
    /// node starts.
    Run,
}

impl NodeScope {
    /// Whether the node declares a scope: it gets a checkout of its own
    /// and its diff is audited.
    pub fn is_declared(&self) -> bool {
        !matches!(self, NodeScope::Unscoped)
    }

    pub fn is_unscoped(&self) -> bool {
        matches!(self, NodeScope::Unscoped)
    }

    /// The globs the workflow wrote, if it wrote any.
    pub fn globs(&self) -> &[ScopeGlob] {
        match self {
            NodeScope::Globs(globs) => globs,
            NodeScope::Unscoped | NodeScope::Run => &[],
        }
    }

    /// What this scope may reach, for a rule that asks whether two nodes
    /// can write the same files before any run exists: the globs as
    /// written, and for `run` anything, since what the run changed is
    /// known only when the node starts.
    pub fn overlap_globs(&self) -> Vec<ScopeGlob> {
        match self {
            NodeScope::Unscoped => Vec::new(),
            NodeScope::Globs(globs) => globs.clone(),
            NodeScope::Run => "**".parse().into_iter().collect(),
        }
    }
}

impl FromIterator<ScopeGlob> for NodeScope {
    /// The globs, or nothing declared when there are none.
    fn from_iter<I: IntoIterator<Item = ScopeGlob>>(globs: I) -> Self {
        let globs: Vec<ScopeGlob> = globs.into_iter().collect();
        match globs.is_empty() {
            true => NodeScope::Unscoped,
            false => NodeScope::Globs(globs),
        }
    }
}

impl<'de> Deserialize<'de> for NodeScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        match Value::deserialize(deserializer)? {
            Value::String(word) if word == "run" => Ok(NodeScope::Run),
            sequence @ Value::Sequence(_) => {
                let globs: Vec<ScopeGlob> =
                    crate::yaml::from_value(sequence).map_err(D::Error::custom)?;
                Ok(globs.into_iter().collect())
            }
            other => Err(D::Error::custom(format!(
                "a scope is a list of globs or `run`, not {}",
                describe(&other)
            ))),
        }
    }
}

impl Serialize for NodeScope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            NodeScope::Unscoped => serializer.collect_seq(std::iter::empty::<&ScopeGlob>()),
            NodeScope::Globs(globs) => globs.serialize(serializer),
            NodeScope::Run => serializer.serialize_str("run"),
        }
    }
}

impl schemars::JsonSchema for NodeScope {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "NodeScope".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "The globs the node may change, or `run`: the paths the run changed since its base",
            "oneOf": [
                { "type": "array", "items": { "type": "string" } },
                { "type": "string", "const": "run" }
            ]
        })
    }
}
