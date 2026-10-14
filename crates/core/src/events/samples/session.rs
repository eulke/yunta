//! The samples of the session's own kinds: an agent session opening and
//! speaking, the writes and stops the engine refused, the formats it ran on
//! writes, the rail's failures and summaries, the run-tool calls it refused,
//! and the service it waited on.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use crate::best_effort::BestEffortStep;
use crate::confinement::{Commands, Worktree};
use crate::events::*;
use crate::port::PermissionProfile;
use crate::rail::hook::RailFailureCause;
use crate::rail::timing::Histogram;
use crate::rail::RailEvent;

pub(crate) fn agent_session_opened() -> AgentSessionOpenedPayload {
    AgentSessionOpenedPayload {
        session_id: "sess-1".into(),
        agent: None,
        effort: None,
        model: Some("mock-model".into()),
        capabilities: Capabilities::default(),
        fence: None,
        confinement: Some(ConfinementRecord {
            profile: PermissionProfile::Edit,
            commands: Commands::Confined,
            worktree: Worktree::ExactFiles,
            network: true,
        }),
        task_id: None,
        continues: None,
    }
}

pub(crate) fn agent_message() -> AgentMessagePayload {
    AgentMessagePayload {
        message_type: AgentMessageType::Usage,
        tool_name: None,
        target: None,
        input_tokens: Some(100),
        output_tokens: Some(50),
        cached_input_tokens: None,
        text: None,
    }
}

pub(crate) fn capability_degraded() -> CapabilityDegradedPayload {
    CapabilityDegradedPayload::new(
        crate::Capability::ResumeSession,
        "mock".into(),
        crate::port::Policy::FreshSession,
    )
}

pub(crate) fn write_refused() -> WriteRefusedPayload {
    WriteRefusedPayload::new(
        "sess-1".into(),
        ToolTarget::of_path(Path::new("docs/readme.md")),
    )
}

pub(crate) fn stop_refused() -> StopRefusedPayload {
    StopRefusedPayload {
        session_id: "sess-1".into(),
        state: crate::sha256_hex(b"sha256:555"),
        why: vec![StopRefusal::CriteriaRed, StopRefusal::OutsideScope],
        task_id: Some("graph-cmd".into()),
    }
}

pub(crate) fn write_formatted() -> WriteFormattedPayload {
    WriteFormattedPayload {
        session_id: "sess-1".into(),
        run: "rustfmt".into(),
        paths: vec!["src/graph.rs".into(), "src/lib.rs".into()],
        changed: vec!["src/graph.rs".into()],
        exit_code: 0,
        duration_ms: 34,
        output: Some(crate::sha256_hex(b"rustfmt printed nothing")),
        timed_out: false,
    }
}

pub(crate) fn rail_failed() -> RailFailedPayload {
    RailFailedPayload {
        session_id: Some("sess-1".into()),
        event: Some(RailEvent::BeforeWrite),
        cause: RailFailureCause::Timeout,
    }
}

pub(crate) fn rail_summary() -> RailSummaryPayload {
    let mut before_write = Histogram::default();
    before_write.observe(Duration::from_millis(12));
    RailSummaryPayload {
        session_id: "sess-1".into(),
        model: Some("mock-model".into()),
        moments: BTreeMap::from([(RailEvent::BeforeWrite, before_write)]),
    }
}

pub(crate) fn run_tool_failed() -> RunToolFailedPayload {
    RunToolFailedPayload {
        session_id: "sess-1".into(),
        tool: crate::RunTool::Submit(crate::ArtifactKind::Questions),
        cause: RunToolFailureCause::ApprovalBlocked,
    }
}

pub(crate) fn run_tool_refused() -> RunToolRefusedPayload {
    RunToolRefusedPayload {
        tool: crate::RunTool::RequestScopeExpansion,
        reason: RunToolRefusal::RequestPending,
    }
}

pub(crate) fn service_unreachable() -> ServiceUnreachablePayload {
    ServiceUnreachablePayload {
        session_id: "sess-1".into(),
        message: "API Error: Can't reach the API server (ENOTFOUND)".to_string(),
    }
}

pub(crate) fn service_reachable() -> ServiceReachablePayload {
    ServiceReachablePayload { waited_ms: 42_000 }
}

/// The sample every test builds the best-effort row from.
pub(crate) fn best_effort_failed() -> BestEffortFailedPayload {
    BestEffortFailedPayload {
        step: BestEffortStep::McpClientClose,
        cause: "the MCP client did not close after its query".to_string(),
    }
}
