//! The keys a workflow reads: at its top, in every node, and in each kind
//! of node; and the node keys it reads no longer, each with what took its
//! place. The audit refuses a key that is not here, and the schema page
//! lists every one that is.

use super::NodeKind;

/// The keys a workflow accepts at its top.
pub const WORKFLOW_KEYS: &[&str] = &[
    "name",
    "description",
    "modes",
    "inputs",
    "node_defaults",
    "nodes",
    "yunta_schema",
    "on_finish",
];

/// The keys every node accepts at its own level; `kind` and the keys
/// that belong to the kind are [`NodeKind`]'s.
pub const NODE_KEYS: &[&str] = &[
    "id",
    "depends_on",
    "scope",
    "runner",
    "runners",
    "agent",
    "artifacts",
    "hooks",
    "on_failure",
    "on_interrupt",
    "description",
    "permissions",
    "network",
    "context",
    "skills",
    "invariant",
    "optional",
    "ask",
    "cli_config",
    "effort",
];

/// Every key a node of `kind` reads: those of every node, `kind`, then
/// its kind's own; `None` for a kind no node can be.
pub(super) fn node_keys(kind: &str) -> Option<Vec<&'static str>> {
    let own = NodeKind::keys(kind)?;
    Some(
        NODE_KEYS
            .iter()
            .copied()
            .chain([NodeKind::TAG])
            .chain(own.iter().copied())
            .collect(),
    )
}

/// Keys an author reaches for that no node accepts, each with the key
/// that expresses the intent.
pub(super) const RETIRED_NODE_KEYS: &[(&str, &str)] = &[
    ("role", "a node names its runner with `runner:`"),
    (
        "interactive",
        "a node that declares `questions` asks them, and whichever surface is \
         watching puts them to a person",
    ),
    (
        "fresh_context",
        "every session starts fresh; `on_interrupt: resume_session` reuses one only when a \
         run resumes",
    ),
];

/// What took the place of `key`, a node key retired, in the words a
/// refusal of it says; `None` for a key never retired.
pub(super) fn retired(key: &str) -> Option<&'static str> {
    RETIRED_NODE_KEYS
        .iter()
        .find(|(retired, _)| *retired == key)
        .map(|(_, instead)| *instead)
}
