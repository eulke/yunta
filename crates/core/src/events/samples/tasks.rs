//! The samples of the task kinds: a task's registration, its status, its
//! check, the deviations it declared or resolved, and a criterion it
//! declared no work of the plan can pass.

use crate::events::*;

pub(crate) fn task_registered() -> TaskRegisteredPayload {
    TaskRegisteredPayload {
        task_id: "graph-cmd".into(),
        criteria: vec![Criterion {
            cmd: "cargo test -p yunta".to_string(),
            r#type: None,
        }],
        scope: vec!["crates/cli/**".into()],
        depends_on: vec![],
    }
}

pub(crate) fn task_status_changed() -> TaskStatusChangedPayload {
    TaskStatusChangedPayload::done("graph-cmd".into(), 42.into(), "deadbeef".into())
}

pub(crate) fn task_check_started() -> TaskCheckStartedPayload {
    TaskCheckStartedPayload {
        task_id: "graph-cmd".into(),
    }
}

pub(crate) fn task_check_answered() -> TaskCheckAnsweredPayload {
    TaskCheckAnsweredPayload {
        task_id: "graph-cmd".into(),
        closes: false,
        results: vec![CriterionResult {
            cmd: "cargo test -p yunta".to_string(),
            exit_code: 1,
            r#type: None,
            reused: false,
            duration_ms: Some(4_200),
            output: None,
            tail: vec!["test graph ... FAILED".to_string()],
            tree: None,
            head: None,
        }],
        outside_scope: vec!["notes.md".into()],
        denied: Vec::new(),
        duration_ms: 4_310,
        waiting: Vec::new(),
    }
}

pub(crate) fn deviation_declared() -> DeviationDeclaredPayload {
    DeviationDeclaredPayload {
        task_id: "graph-cmd".into(),
        from: DepartsFrom::Shape("GraphFormat".to_string()),
        planned: "a `Dot` variant".to_string(),
        instead: "only `Mermaid`".to_string(),
        why: "the scope leaves the renderer out".to_string(),
    }
}

pub(crate) fn deviation_resolved() -> DeviationResolvedPayload {
    DeviationResolvedPayload {
        task_id: "graph-cmd".into(),
        accepted: false,
        said: Some("add `Dot` too".to_string()),
        respecified_by: None,
    }
}

pub(crate) fn criterion_declared_unpassable() -> CriterionUnpassablePayload {
    CriterionUnpassablePayload {
        task_id: "graph-cmd".into(),
        criterion: "cargo test -p yunta".to_string(),
        exit_code: 101,
        why: "the crate it runs is written by another task".to_string(),
    }
}
