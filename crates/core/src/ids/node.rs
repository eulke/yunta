//! A node's id: the name a workflow gives a node, and the form its fan-out
//! siblings take.

use std::borrow::{Borrow, Cow};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{is_node_id, string_id, InvalidId, RunnerName, FAN_OUT_SEPARATOR, NODE_RULE};

string_id!(
    /// A node's id, unique within a workflow: `^[A-Za-z][A-Za-z0-9_-]*$`
    /// as authored. A fan-out sibling — one of the nodes the manifest
    /// expands a `runners:` node into — is `<base>@<runner>`; authored
    /// YAML never spells that form, the manifest does.
    NodeId, what = "node id", rule = NODE_RULE, check = is_node_id
);

impl NodeId {
    /// The id of the sibling of `base` that runs under `runner`. `base`
    /// contributes its authored id, so the result is one `@` deep even
    /// when `base` is itself a sibling.
    pub fn fan_out(base: &NodeId, runner: &RunnerName) -> NodeId {
        NodeId(Cow::Owned(format!(
            "{}{}{runner}",
            base.base_str(),
            FAN_OUT_SEPARATOR as char
        )))
    }

    /// Whether this is a fan-out sibling (`<base>@<runner>`).
    pub fn is_fan_out(&self) -> bool {
        self.0.contains(FAN_OUT_SEPARATOR as char)
    }

    /// The authored id: the part before `@` for a sibling, the id itself
    /// otherwise.
    pub fn base(&self) -> NodeId {
        if self.is_fan_out() {
            NodeId(Cow::Owned(self.base_str().to_string()))
        } else {
            self.clone()
        }
    }

    /// The runner a fan-out sibling runs under; `None` for an authored
    /// node.
    pub fn runner(&self) -> Option<RunnerName> {
        self.0
            .split_once(FAN_OUT_SEPARATOR as char)
            .map(|(_, runner)| RunnerName(Cow::Owned(runner.to_string())))
    }

    fn base_str(&self) -> &str {
        self.0
            .split_once(FAN_OUT_SEPARATOR as char)
            .map_or(self.as_str(), |(base, _)| base)
    }
}
