use std::fmt;

use serde::{Deserialize, Serialize};

/// A node identifier, unique within a workflow (Contrato §2, §3).
///
/// Newtyped per CLAUDE.md ("Parse, don't validate"): a bare `String` could
/// be any string in the domain; a `NodeId` can only be a node identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for NodeId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

impl From<String> for NodeId {
    fn from(id: String) -> Self {
        Self(id)
    }
}
