//! The shared view of a `coordination: blackboard` group: what a member
//! may read while the group runs, and what the group leaves behind when
//! it joins.
//!
//! The two answer to one rule. While siblings are live, a member reads
//! only its own posts — reading a sibling hot would make the outcome
//! depend on which session got there first. Everything a group produced
//! becomes readable at the join, in an order derived from content rather
//! than from arrival, so two runs whose posts raced differently produce
//! byte-identical output.

use serde_json::{json, Value};
use yunta_core::events::{EventPayload, Finding, StoredEvent};
use yunta_core::NodeId;

use super::session::{RunToolError, SessionTools};

/// Post-join consolidation, pure over the log: every
/// `finding_posted` authored by a member of the group, sorted by
/// `(node, finding id, title)` — **never by arrival order**, which is
/// exactly what makes two runs whose posts raced differently produce
/// byte-identical output. Written as the group's own node-output at
/// its close, consumable by a node after the `parallel`
/// (`context: [{node-output: {node: <group_id>}}]`) — never between
/// siblings hot.
pub fn consolidate_blackboard(events: &[StoredEvent], members: &[NodeId]) -> String {
    let mut entries: Vec<(String, Finding)> = events
        .iter()
        .filter_map(|event| {
            let node = event.node_id.as_ref()?;
            if !members.contains(node) {
                return None;
            }
            match event.payload() {
                Some(EventPayload::FindingPosted(p)) => Some((node.to_string(), p.finding.clone())),
                _ => None,
            }
        })
        .collect();
    entries.sort_by(|a, b| (&a.0, &a.1.id, &a.1.title).cmp(&(&b.0, &b.1.id, &b.1.title)));
    let rendered: Vec<Value> = entries
        .into_iter()
        .map(|(node, finding)| {
            let mut object = serde_json::to_value(&finding)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            object.insert("node".to_string(), Value::String(node));
            Value::Object(object)
        })
        .collect();
    yunta_core::yaml::to_string(&rendered).unwrap_or_default()
}

impl SessionTools {
    /// Whether this session's node sits in a `coordination: blackboard`
    /// group — the mount rule for `yunta_get_blackboard`.
    pub(super) fn in_blackboard_group(&self) -> bool {
        self.host.blackboard_members.contains_key(&self.node)
    }

    pub(super) async fn get_blackboard(&self) -> Result<String, RunToolError> {
        if !self.in_blackboard_group() {
            return Err(RunToolError::NotInBlackboardGroup);
        }
        // While the group runs, only this node's OWN posts —
        // reading a sibling hot would make the outcome depend on
        // arrival order, not content. Siblings' posts arrive through
        // the group's post-join consolidation, never through here.
        let own: Vec<Finding> = self
            .events()
            .await?
            .into_iter()
            .filter(|event| event.node_id.as_ref() == Some(&self.node))
            .filter_map(|event| match event.payload() {
                Some(EventPayload::FindingPosted(p)) => Some(p.finding.clone()),
                _ => None,
            })
            .collect();
        serde_json::to_string_pretty(&json!({
            "note": "your own posts only — siblings' posts become readable after the \
                     group's join, through its consolidated output",
            "findings": own,
        }))
        .map_err(|source| RunToolError::Render { source })
    }
}
