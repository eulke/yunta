//! The words a session's missing capability is said in: a capability the
//! adapter does not declare, and the policy the engine applied in its place.

use yunta_core::port::PolicyApplied;
use yunta_core::text::{detailed, one_line};
use yunta_core::{AdapterId, Capability};

/// A capability the adapter does not declare, and the policy the engine
/// applied in its place.
pub(super) fn degraded(
    capability: &Capability,
    adapter: &AdapterId,
    policy: &PolicyApplied,
) -> String {
    detailed(
        format!("{} not declared by {adapter}", capability.as_str()),
        &one_line(&yunta_render::policy::applied(policy)),
    )
}
