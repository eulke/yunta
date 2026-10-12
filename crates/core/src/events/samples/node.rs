//! The samples of the node's own kinds: the runner it resolved, its start,
//! its checks, how it finished or failed, and where it was rerouted.

use std::collections::BTreeMap;

use crate::events::*;
use crate::RunnerCandidate;

pub(crate) fn runner_resolved() -> RunnerResolvedPayload {
    RunnerResolvedPayload {
        runner: "executor".into(),
        chosen: RunnerCandidate {
            adapter: "mock".into(),
            model: "mock-model".into(),
            agent: None,
            effort: None,
        },
        discarded: vec![],
    }
}

pub(crate) fn node_started() -> NodeStartedPayload {
    NodeStartedPayload::attempt(1)
}

pub(crate) fn context_assembled() -> ContextAssembledPayload {
    ContextAssembledPayload {
        task_id: None,
        sources: vec![ContextSourceRef {
            source_id: "files:docs".to_string(),
            kind: "files".to_string(),
            content_hash: crate::sha256_hex(b"sha256:333"),
            absent: vec!["docs/architecture.md".to_string()],
        }],
        segment_hashes: BTreeMap::from([("stable".to_string(), crate::sha256_hex(b"222"))]),
    }
}

pub(crate) fn criteria_checked() -> CriteriaCheckedPayload {
    CriteriaCheckedPayload {
        task_id: "graph-cmd".into(),
        phase: Phase::Pre,
        results: vec![CriterionResult {
            cmd: "cargo test -p yunta".to_string(),
            exit_code: 1,
            r#type: None,
            reused: false,
            duration_ms: Some(4_200),
            output: None,
            tail: Vec::new(),
            tree: None,
            head: None,
        }],
        waiting: Vec::new(),
    }
}

pub(crate) fn scope_checked() -> ScopeCheckedPayload {
    ScopeCheckedPayload {
        task_id: Some("graph-cmd".into()),
        diff: vec!["crates/cli/src/graph.rs".into()],
        violations: vec![],
        denied: Vec::new(),
    }
}

pub(crate) fn node_finished() -> NodeFinishedPayload {
    NodeFinishedPayload::new(
        NodeOutcome::Summary("criteria green".into()),
        TokenUsage {
            input: 10,
            output: 5,
            cached: None,
        },
    )
}

pub(crate) fn node_failed() -> NodeFailedPayload {
    NodeFailedPayload::new(
        Failure::message("criteria red"),
        true,
        TokenUsage::default(),
    )
}

pub(crate) fn hook_executed() -> HookExecutedPayload {
    HookExecutedPayload {
        phase: HookPhase::After,
        command: "cargo fmt".to_string(),
        exit_code: 0,
        output: None,
        tail: Vec::new(),
    }
}

pub(crate) fn node_rerouted() -> NodeReroutedPayload {
    NodeReroutedPayload::new(
        "fix-lint".into(),
        Failure::message("clippy failed"),
        RerouteOrigin::OnFailure,
        Some(1),
        Some(2),
    )
}

pub(crate) fn pull_request_opened() -> PullRequestOpenedPayload {
    PullRequestOpenedPayload {
        url: "https://github.example/pr/7".to_string(),
        number: 7,
        head: "yunta/run/01".to_string(),
        base: "main".to_string(),
    }
}
