//! `hooks:` and `on_failure:` — what runs around a node and where it
//! re-routes when it fails.

use serde::{Deserialize, Serialize};

use super::RunCommand;
use crate::ids::NodeId;

/// `hooks: {before, after}` — commands the engine runs around a node's own
/// work: `before` as the node starts, `after` before its close verifies
/// anything, so what an `after` hook rewrites is what the criteria, the
/// scope and the artifacts are judged on. Grouped because the two only
/// mean something together and share their rules: commands only, never an
/// agent; safe to repeat, since they run again on every retry and every
/// resume; each run recorded as `hook_executed`; their edits held to the
/// node's scope like any other. A workflow declares them — on a node, or
/// for every node in `node_defaults` — and no config layer adds one.
/// Business logic, or anything that takes long, is a `bash` node with a
/// state of its own instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<HookStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<HookStep>,
}

/// One hook command: a script the workflow wrote, or a project command by
/// name. `timeout_seconds` bounds it — in seconds, the scale of a hook,
/// where a whole session's bound is in minutes — and absent, nothing
/// does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HookStep {
    pub run: RunCommand,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// Distinct from a node's own `on_failure.goto` re-routing — a hook
    /// only ever fails or warns, never re-routes.
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

/// `on_failure: {goto, max_reroutes}` — where a failed node sends the run
/// to be corrected. Re-routes are a second set of edges beside
/// `depends_on`, which stays acyclic: control goes to `goto`, and once it
/// and its subgraph complete, the failed node runs again. `max_reroutes`
/// is mandatory and counted per failed node, and when it runs out the run
/// pauses for a person: a re-route without an explicit cap is how a
/// correction cycle turns infinite, so the schema refuses it. A failure no
/// correction can change — a key the run's frozen config leaves unset, a
/// limit that resets at a known time — is never re-routed. Distinct from a
/// hook's own `on_failure: fail|warn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OnFailure {
    pub goto: NodeId,
    pub max_reroutes: u32,
}
