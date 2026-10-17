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

/// The most permissive `scope_expansion.mode` a permissions layer allows a
/// node to declare: a node may declare any mode at least as strict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct ScopeCeiling(pub ScopeExpansionMode);

impl ScopeCeiling {
    /// Whether a node's declared mode stays within this ceiling.
    pub fn allows(self, mode: ScopeExpansionMode) -> bool {
        mode.strictness() >= self.0.strictness()
    }

    /// The strictest of two ceilings, as the layered merge keeps it.
    pub fn stricter(self, other: Self) -> Self {
        if self.0.strictness() >= other.0.strictness() {
            self
        } else {
            other
        }
    }

    /// Whether a person may widen a node's scope: every ceiling but `deny` allows it.
    pub fn lets_a_person_grant(self) -> bool {
        self.0 != ScopeExpansionMode::Deny
    }
}
