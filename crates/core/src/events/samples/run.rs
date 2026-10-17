//! The samples of the run's own kinds: its birth, the suite it measured and
//! the commands it held, its promotion, the machine's suspension, its pauses
//! and resumes, its moves, and its close.

use std::collections::BTreeMap;

use crate::events::*;

pub(crate) fn run_created() -> RunCreatedPayload {
    RunCreatedPayload {
        checkout: None,
        manifest_hash: crate::sha256_hex(b"sha256:abc"),
        inputs: BTreeMap::new(),
        mode: "default".into(),
        promoted_from: None,
        yunta_schema: None,
        base_branch: "main".to_string(),
        base_commit: "deadbeef".into(),
        environment: None,
        left_out: Vec::new(),
        opens_on_base: false,
    }
}

pub(crate) fn baseline_captured() -> BaselineCapturedPayload {
    BaselineCapturedPayload {
        command: "cargo test --workspace".to_string(),
        results: BaselineResults {
            exit_code: 0,
            summary: "12 passed".to_string(),
        },
        hash: crate::sha256_hex(b"sha256:def"),
        origin: BaselineOrigin::Measured,
        tree: None,
        duration_ms: None,
    }
}

pub(crate) fn baseline_held() -> BaselineHeldPayload {
    BaselineHeldPayload {
        name: "lint".into(),
        command: "cargo clippy --workspace".to_string(),
        results: BaselineResults {
            exit_code: 0,
            summary: "no warnings".to_string(),
        },
        origin: BaselineOrigin::Measured,
        tree: Some("deadbeef".into()),
        duration_ms: Some(1_200),
    }
}

pub(crate) fn promotion_signaled() -> PromotionSignaledPayload {
    PromotionSignaledPayload::new(
        PromotionCause {
            node: crate::NodeId::from("implement"),
            goto: crate::NodeId::from("fix-lint"),
            failure: None,
        },
        vec![Fact::labelled("criteria", "log")].into(),
        "beta".into(),
    )
}

pub(crate) fn host_suspended() -> HostSuspendedPayload {
    HostSuspendedPayload {
        slept_ms: 3_600_000,
    }
}

pub(crate) fn run_paused() -> RunPausedPayload {
    RunPausedPayload::new(&PauseReason::Recorded {
        sentence: "gate waiting".into(),
    })
}

pub(crate) fn run_resumed() -> RunResumedPayload {
    RunResumedPayload {
        checkout: None,
        policies: Vec::new(),
        environment: None,
        tree: Some("deadbeef".into()),
    }
}

pub(crate) fn run_moved() -> RunMovedPayload {
    RunMovedPayload {
        checkout: std::path::PathBuf::from("/pool/slot-2"),
    }
}

pub(crate) fn run_finished() -> RunFinishedPayload {
    RunFinishedPayload {
        terminal_state: TerminalState::Done,
        metrics: RunMetrics {
            cptv: Some(0.42),
            tokens: TokenUsage {
                input: 1000,
                output: 500,
                cached: Some(200),
            },
        },
        closed_by: None,
    }
}
