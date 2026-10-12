//! The samples of the scope kinds: a request to write outside a scope, the
//! answer to it, and the scope a task derives from its shapes.

use crate::events::*;
use crate::ScopeExpansionMode;

pub(crate) fn scope_expansion_requested() -> ScopeExpansionRequestedPayload {
    ScopeExpansionRequestedPayload {
        task_id: Some("graph-cmd".into()),
        paths: vec!["crates/cli/src/**".into()],
        reason: "need to touch main.rs too".to_string(),
        proposed_criterion: None,
        proposed_criterion_precheck: None,
    }
}

pub(crate) fn scope_expansion_granted() -> ScopeExpansionGrantedPayload {
    ScopeExpansionGrantedPayload {
        task_id: Some("graph-cmd".into()),
        decided_by: Decider::Rule,
        mode: ScopeExpansionMode::Rules,
        count_this_run: 1,
        paths: vec!["crates/cli/src/**".into()],
    }
}

pub(crate) fn scope_expansion_denied() -> ScopeExpansionDeniedPayload {
    ScopeExpansionDeniedPayload {
        task_id: Some("graph-cmd".into()),
        decided_by: Decider::Person { id: "eulke".into() },
        mode: ScopeExpansionMode::Ask,
        count_this_run: 2,
        denial_reason: Some("out of declared scope".to_string()),
    }
}

pub(crate) fn scope_derived() -> ScopeDerivedPayload {
    ScopeDerivedPayload {
        task_id: "graph-cmd".into(),
        paths: vec!["crates/cli/src/commands/run.rs".into()],
        shapes: vec!["build_manifest".to_string()],
        common: vec!["run".to_string()],
        at: "deadbeef".into(),
    }
}
