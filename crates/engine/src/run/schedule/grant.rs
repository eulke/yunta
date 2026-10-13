//! Whether a person is offered a grant for a failure that asks for scope: the
//! menu offers one only where the node's close would apply it whole.

use yunta_core::events::Failure;
use yunta_core::expansion::node_offer;
use yunta_core::Node;

use super::Board;

impl Board<'_> {
    /// Whether a person may widen `node`'s scope by what `failure` needs:
    /// the failure is about scope, the node is not read-only — its word is
    /// the whole of its ceiling — and the run's permission ceiling lets a
    /// person grant. Offered only when the close would apply the grant
    /// whole, which is the same answer [`node_offer`] gives the close. An
    /// owed failure widens each task by its own request, so it is offered
    /// when one of those requests would be applied.
    pub(super) fn grantable(&self, node: &Node, failure: &Failure) -> bool {
        let widenable = self.policy.grants_scope && node.writes_worktree() && failure.wants_scope();
        let protected = &self.policy.protected;
        match failure {
            Failure::ScopeOwed { owed } => owed
                .iter()
                .any(|request| node_offer(&request.paths, protected, widenable).is_ok()),
            _ => failure
                .scope_wanted()
                .is_ok_and(|wanted| node_offer(&wanted, protected, widenable).is_ok()),
        }
    }
}
