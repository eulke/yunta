//! Policy vocabulary shared by the schema and the log: what a workflow
//! declares, what the config caps, and what an event records use one
//! set of values.

use serde::{Deserialize, Serialize};

/// How a loop node treats an agent's request to widen its scope.
/// Default `Deny`: a node that omits `scope_expansion:` entirely gets
/// the same behavior as one that declares it with no `mode:` — no
/// expansions, every request becomes a finding without interrupting.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ScopeExpansionMode {
    Rules,
    Ask,
    #[default]
    Deny,
}

impl ScopeExpansionMode {
    /// The severity order the layered ceiling compares by —
    /// `rules` is the most permissive (auto-grants), `deny` the least.
    /// A higher number never grants what a lower one would refuse.
    pub fn strictness(self) -> u8 {
        match self {
            ScopeExpansionMode::Rules => 0,
            ScopeExpansionMode::Ask => 1,
            ScopeExpansionMode::Deny => 2,
        }
    }

    /// The YAML spelling, for diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeExpansionMode::Rules => "rules",
            ScopeExpansionMode::Ask => "ask",
            ScopeExpansionMode::Deny => "deny",
        }
    }
}
