//! Which facts this domain records, and what each is called on the wire.

use super::payloads::*;

/// A node asking to write outside the scope it declared, and the answer — one variant per kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ScopeEvent {
    Requested(ScopeExpansionRequestedPayload),
    Granted(ScopeExpansionGrantedPayload),
    Denied(ScopeExpansionDeniedPayload),
}

impl ScopeEvent {
    /// Every kind this domain declares, as persisted.
    pub const KINDS: &'static [&'static str] = &[
        "scope_expansion_requested",
        "scope_expansion_granted",
        "scope_expansion_denied",
    ];

    /// The persisted `kind` string of this fact.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Requested(_) => "scope_expansion_requested",
            Self::Granted(_) => "scope_expansion_granted",
            Self::Denied(_) => "scope_expansion_denied",
        }
    }

    /// The shape version of this kind. Every kind starts at 1 and a
    /// version is per kind, never per domain and never global: a kind
    /// that gains an incompatible shape becomes a new name, and only
    /// that one moves.
    pub fn schema_version(&self) -> u32 {
        1
    }
}
