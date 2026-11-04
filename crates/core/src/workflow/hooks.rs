//! `hooks:` and `on_failure:` — what runs around a node and where it
//! reroutes when it fails.

use serde::{Deserialize, Serialize};

use super::RunCommand;
use crate::ids::NodeId;

/// `hooks: {before, after}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<HookStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<HookStep>,
}

/// One hook command. `timeout_seconds` is unenforced (no timeout) when
/// absent — additive over the earlier engine, which never had one.
/// Field name/units aren't pinned by the spec's prose ("timeout corto
/// configurable"); seconds fit hook-scale commands better than the
/// minutes granularity `defaults.timeout_minutes` uses for whole agent
/// sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HookStep {
    pub run: RunCommand,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// Distinct from a node's own `on_failure.goto` rerouting — a hook
    /// only ever fails or warns, never reroutes.
    #[serde(default, skip_serializing_if = "is_default_hook_failure_policy")]
    pub on_failure: HookFailurePolicy,
}

fn is_default_hook_failure_policy(policy: &HookFailurePolicy) -> bool {
    *policy == HookFailurePolicy::default()
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HookFailurePolicy {
    #[default]
    Fail,
    Warn,
}

/// `on_failure: {goto, max_reroutes}` — node-level rerouting.
/// `max_reroutes` is mandatory: a reroute without an explicit cap
/// is how a correction cycle turns infinite, so the schema refuses it.
/// Distinct from a hook's own `on_failure: fail|warn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OnFailure {
    pub goto: NodeId,
    pub max_reroutes: u32,
}
