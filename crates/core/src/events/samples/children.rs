//! The samples of the child kinds: one iteration of a loop, and a child run
//! created and then finished.

use crate::events::*;

pub(crate) fn loop_iteration() -> LoopIterationPayload {
    LoopIterationPayload {
        iteration: 3,
        until_result: false,
    }
}

pub(crate) fn child_run_created() -> ChildRunCreatedPayload {
    ChildRunCreatedPayload {
        child_run_id: "run-child-1".into(),
        child_workflow_hash: crate::sha256_hex(b"sha256:444"),
    }
}

pub(crate) fn child_run_finished() -> ChildRunFinishedPayload {
    ChildRunFinishedPayload::new(
        "run-child-1".into(),
        crate::sha256_hex(b"sha256:444"),
        TerminalState::Done,
        TokenUsage::default(),
    )
}
