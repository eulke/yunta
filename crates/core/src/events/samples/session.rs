//! The samples of the session's own kinds: an agent session opening and
//! speaking, the writes and stops the engine refused, the formats it ran on
//! writes, the run-tool calls it refused, and the service it waited on.

use std::path::Path;

use crate::confinement::{Commands, Worktree};
use crate::events::*;
use crate::port::PermissionProfile;

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
