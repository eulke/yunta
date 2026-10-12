//! Which facts this domain records.

use super::payloads::*;

/// A node asking to write outside the scope it declared, and the answer;
/// and what a task may write beyond it because a shape it owns is named
/// there — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ScopeEvent {
    Requested(ScopeExpansionRequestedPayload),
    Granted(ScopeExpansionGrantedPayload),
    Denied(ScopeExpansionDeniedPayload),
    Derived(ScopeDerivedPayload),
}
