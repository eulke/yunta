//! What the adapters could not do, folded once.
//!
//! A capability the engine consulted and an adapter does not declare is
//! a degradation the log records, and until now exactly one surface read
//! it: the receipt, `stats` and `verify` never mentioned it at all.
//! Every surface that counts or lists degradations reads them here.

use chrono::{DateTime, Utc};

use crate::events::meta::EventMeta;
use crate::events::session::kinds::SessionEvent;
use crate::ids::{AdapterId, NodeId};
use crate::Capability;

/// One `capability_degraded`: the capability the engine consulted, the
/// adapter that does not declare it, and the policy applied instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Degradation {
    pub capability: Capability,
    pub adapter: AdapterId,
    pub policy: String,
    pub node: Option<NodeId>,
    pub at: DateTime<Utc>,
}

/// Every degradation the run recorded, in log order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DegradationLedger {
    all: Vec<Degradation>,
}

impl DegradationLedger {
    /// Every degradation, oldest first.
    pub fn all(&self) -> &[Degradation] {
        &self.all
    }

    /// Whether the run recorded any.
    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    /// Whether the run already said this capability was missing. A
    /// fallback the whole run works under is stated once: repeating it
    /// per node or per session says nothing new and buries what does.
    pub fn already_stated(&self, capability: Capability) -> bool {
        self.all
            .iter()
            .any(|degradation| degradation.capability == capability)
    }

    /// Folds one session-domain event.
    pub fn apply(&mut self, event: &SessionEvent, meta: &EventMeta<'_>) {
        match event {
            SessionEvent::CapabilityDegraded(p) => self.all.push(Degradation {
                capability: p.capability,
                adapter: p.adapter.clone(),
                policy: p.policy_applied().to_string(),
                node: meta.node.cloned(),
                at: meta.at,
            }),
            // A session opening and what it says while it runs belong to
            // the node that opened it: `NodeLedger::apply_session` holds
            // them, because an attempt is what bounds a session's life.
            SessionEvent::Opened(_)
            | SessionEvent::Message(_)
            | SessionEvent::WriteRefused(_)
            | SessionEvent::RunToolFailed(_) => {}
        }
    }
}
