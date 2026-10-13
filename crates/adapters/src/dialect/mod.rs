//! The CLIs a session can stand for. A dialect is one CLI's name, and every
//! choice that differs between CLIs is a match on it, made here and nowhere
//! else. The real adapters and the mock both plan a launch through the same
//! function, so a session's argv, environment, scratch files, mounts and
//! fence are the same whichever of them runs it.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use yunta_core::durable::{self, Durability};
use yunta_core::events::FailureCause;
use yunta_core::fence::Coverage;
use yunta_core::port::session::SessionEnv;
use yunta_core::port::{HookReply, RailCodec, SessionRequest};
use yunta_core::process::subprocess::LineParser;
use yunta_core::rail::{RailAnswer, RailCommand, RailEndpoint, RailEvent};
use yunta_core::{
    AdapterError, AdapterId, AdapterSettings, Capabilities, ModelName, Result, ScopeGlob,
    SessionId, ToolNaming, Unbuildable,
};

use crate::failure::classify;
use crate::mock::{MockOutcome, MockStep};
use crate::mount::MountPlan;
use crate::{claude_code, codex};

/// The CLI a session stands for. Every fixture names one, and the real
/// adapters are the same two names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DialectName {
    ClaudeCode,
    Codex,
}

impl DialectName {
    /// The one planner per CLI, which the real launch and the mock both call.
    pub(crate) fn plan(
        self,
        settings: &AdapterSettings,
        req: &SessionRequest,
        resume: Option<&SessionId>,
    ) -> Result<Plan> {
        match self {
            Self::ClaudeCode => claude_code::plan(settings, req, resume),
            Self::Codex => codex::plan(settings, req, resume),
        }
    }

    /// What this CLI declares it can do.
    pub(crate) fn capabilities(self) -> Capabilities {
        match self {
            Self::ClaudeCode => claude_code::capabilities(),
            Self::Codex => codex::capabilities(),
        }
    }

    /// The files of a checkout this CLI loads configuration from that runs
    /// code or mounts tools, which no session of any run writes.
    pub(crate) fn own_config(self) -> Vec<ScopeGlob> {
        match self {
            Self::ClaudeCode => claude_code::own_config(),
            Self::Codex => codex::own_config(),
        }
    }

    /// The codec of this CLI's hooks: `Some` exactly when `capabilities()`
    /// declares the `ToolCalls` fence, which both CLIs do.
    pub(crate) fn rail_codec(self) -> Option<&'static dyn RailCodec> {
        match self {
            Self::ClaudeCode => Some(&claude_code::rail::ClaudeRailCodec),
            Self::Codex => Some(&codex::rail::CodexRailCodec),
        }
    }

    /// The lines this CLI prints for one scripted step, written from its own format.
    pub(crate) fn render(self, step: &MockStep, naming: ToolNaming) -> Vec<String> {
        match self {
            Self::ClaudeCode => claude_code::wire::render(step, naming),
            Self::Codex => codex::wire::render(step, naming),
        }
    }

    /// The lines this CLI prints first, which open its session.
    pub(crate) fn render_open(
        self,
        session_id: &SessionId,
        model: Option<&ModelName>,
    ) -> Vec<String> {
        match self {
            Self::ClaudeCode => claude_code::wire::open(session_id, model),
            Self::Codex => codex::wire::open(session_id, model),
        }
    }

    /// The lines this CLI prints to close a session that played its whole
    /// script, from the outcome and the usage its steps spent.
    pub(crate) fn render_close(self, outcome: &MockOutcome, usage: &[MockStep]) -> Vec<String> {
        match self {
            Self::ClaudeCode => claude_code::wire::close(outcome),
            Self::Codex => codex::wire::close(outcome, usage),
        }
    }

    /// The stdin this CLI sends its hook for `event`. A write names `path`.
    pub(crate) fn hook_stdin(self, event: RailEvent, session: &SessionId, path: &Path) -> Vec<u8> {
        match self {
            Self::ClaudeCode => claude_code::wire::hook_stdin(event, session, path),
            Self::Codex => codex::wire::hook_stdin(event, session, path),
        }
    }

    /// The answer this CLI takes from a hook's reply for `event`: the reading
    /// that undoes the codec's `encode`, as the CLI itself reads the reply.
    pub(crate) fn read_reply(self, event: RailEvent, reply: &HookReply) -> RailAnswer {
        match self {
            Self::ClaudeCode => claude_code::rail::read_reply(event, reply),
            Self::Codex => codex::rail::read_reply(event, reply),
        }
    }

    /// The reader of this CLI's stream, for a session launched with `plan`.
    pub(crate) fn reader(self, plan: &Plan) -> Box<dyn LineParser> {
        match self {
            Self::ClaudeCode => Box::new(claude_code::ClaudeParser::new(plan.coverage.clone())),
            Self::Codex => Box::new(codex::CodexParser::new(plan.coverage.clone())),
        }
    }

    /// The cause a failure message reads as under this CLI. A Claude limit
    /// is read from the stream's fields and never from the text, so the text
    /// reads only as a refusal; Codex reads its limit phrases from the text.
    pub(crate) fn failure_cause(self, message: &str) -> FailureCause {
        match self {
            Self::ClaudeCode => classify(message),
            Self::Codex => codex::failure_cause(message),
        }
    }

    /// The `adapter_settings` keys this CLI reads.
    pub(crate) fn known_settings(self) -> &'static [&'static str] {
        match self {
            Self::ClaudeCode => claude_code::settings::KNOWN,
            Self::Codex => codex::settings::KNOWN,
        }
    }
}

/// One launch as data: what the CLI is asked, before anything is written.
pub(crate) struct Plan {
    pub args: Vec<String>,
    pub env: SessionEnv,
    /// The files the launch writes first, each with the bytes it holds.
    pub scratch: Vec<(PathBuf, Vec<u8>)>,
    pub mounts: MountPlan,
    pub coverage: Coverage,
}

impl Plan {
    /// Writes the scratch files, each atomically under its own name, after
    /// creating the directory above it when that is missing. The mounts are
    /// not written here: `MountPlan::install` makes them.
    pub(crate) fn write_scratch(&self, adapter: &'static AdapterId) -> Result<()> {
        for (path, bytes) in &self.scratch {
            let io = |source: std::io::Error| AdapterError::Adapter {
                adapter: adapter.clone(),
                message: format!("could not write {}: {source}", path.display()),
            };
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            // blocking: one small write by the launching task, before the CLI
            // that reads the file starts.
            durable::replace(path, bytes, Durability::Atomic).map_err(io)?;
        }
        Ok(())
    }
}

/// The refusal every launch makes first, before it stages or writes anything:
/// a session whose hooks cannot reach the engine opens nothing.
pub(crate) fn require_hook<'a>(
    id: &AdapterId,
    req: &'a SessionRequest,
) -> Result<(&'a RailCommand, &'a RailEndpoint)> {
    match (&req.rail_command, &req.rail) {
        (Some(command), Some(endpoint)) => Ok((command, endpoint)),
        _ => Err(AdapterError::FenceUnbuildable {
            adapter: id.clone(),
            source: Unbuildable::HookUnavailable,
        }),
    }
}
