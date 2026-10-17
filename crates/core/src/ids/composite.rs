//! The composite identifiers: a pack's reference, `publisher/name`, and a
//! GitHub repository, `owner/name`. Each one is two path segments, checked
//! by one rule and printed and serialized the same way.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{is_path_segment, InvalidId, PackName, Publisher};

/// A pack's identity, `publisher/name`: what `yunta run acme/review`
/// and `use: acme/qa-review` name, and the key of `yunta.lock`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackRef {
    publisher: Publisher,
    name: PackName,
}

const PACK_REF_RULE: &str = "`publisher/name`, each one path segment";

impl PackRef {
    pub fn new(publisher: Publisher, name: PackName) -> Self {
        PackRef { publisher, name }
    }

    pub fn publisher(&self) -> &Publisher {
        &self.publisher
    }

    pub fn name(&self) -> &PackName {
        &self.name
    }
}

impl fmt::Display for PackRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.publisher, self.name)
    }
}

impl FromStr for PackRef {
    type Err = InvalidId;

    fn from_str(value: &str) -> Result<Self, InvalidId> {
        let invalid = || InvalidId {
            what: "pack reference",
            value: value.to_string(),
            rule: PACK_REF_RULE,
        };
        let (publisher, name) = value.split_once('/').ok_or_else(invalid)?;
        Ok(PackRef {
            publisher: publisher.parse().map_err(|_| invalid())?,
            name: name.parse().map_err(|_| invalid())?,
        })
    }
}

impl TryFrom<String> for PackRef {
    type Error = InvalidId;

    fn try_from(value: String) -> Result<Self, InvalidId> {
        value.parse()
    }
}

impl Serialize for PackRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PackRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for PackRef {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PackRef".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[^/]+/[^/]+$",
            "description": format!("A pack reference: {}", PACK_REF_RULE),
        })
    }
}

/// A GitHub repository, `owner/name`: the two path segments every
/// forge URL is built from, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitHubRepo {
    owner: String,
    name: String,
}

const GITHUB_REPO_RULE: &str = "`owner/name`, each one path segment";

impl GitHubRepo {
    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for GitHubRepo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

impl FromStr for GitHubRepo {
    type Err = InvalidId;

    fn from_str(value: &str) -> Result<Self, InvalidId> {
        let invalid = || InvalidId {
            what: "GitHub repository",
            value: value.to_string(),
            rule: GITHUB_REPO_RULE,
        };
        let (owner, name) = value.split_once('/').ok_or_else(invalid)?;
        if !is_path_segment(owner) || !is_path_segment(name) {
            return Err(invalid());
        }
        Ok(GitHubRepo {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }
}

impl TryFrom<String> for GitHubRepo {
    type Error = InvalidId;

    fn try_from(value: String) -> Result<Self, InvalidId> {
        value.parse()
    }
}

impl Serialize for GitHubRepo {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for GitHubRepo {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for GitHubRepo {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "GitHubRepo".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[^/]+/[^/]+$",
            "description": format!("A GitHub repository: {}", GITHUB_REPO_RULE),
        })
    }
}
