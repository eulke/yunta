//! What a scripted session writes, which of it its fence or its confinement
//! keeps out, and how the writes its CLI judges one by one are asked of the
//! engine.

use std::path::{Path, PathBuf};
use std::time::Duration;

use yunta_core::fence::{Fence, Verdict};
use yunta_core::port::SessionRequest;
use yunta_core::rail::{RailAnswer, RailEndpoint, RailEvent};
use yunta_core::{AdapterError, FenceLevel, Result, SessionId};

use super::fixture::{MockEffect, SessionScript, Via};
use super::{MockAdapter, ID};
use crate::dialect::DialectName;

/// The longest a mock hook waits for the engine's answer. The engine judges
/// a call in moments, so this bounds only a listener that never answers.
const HOOK_DEADLINE: Duration = Duration::from_secs(30);

/// A session's effects, each asked of the engine before it is written, as
/// the CLI's own write hook asks.
pub(super) struct Asked {
    pub(super) dialect: DialectName,
    pub(super) rail: RailEndpoint,
    pub(super) cwd: PathBuf,
    pub(super) effects: Vec<MockEffect>,
    /// Whether the engine formats what is written: each write that lands
    /// is then told to it, as a CLI's after-write hook does.
    pub(super) formats: bool,
}

impl Asked {
    /// Asks the engine about each tool effect and writes the ones it allows.
    /// A write made through a shell command reaches no write hook, so the
    /// command effects handed here, which have landed, are written as they
    /// are made, with nothing asked and nothing reported.
    pub(super) async fn written(&self, session: &SessionId) -> std::result::Result<(), String> {
        for effect in &self.effects {
            if effect.via == Via::Command {
                write_effect(&self.cwd, effect).map_err(|e| e.to_string())?;
                continue;
            }
            let answer = ask_hook(
                self.dialect,
                &self.rail,
                RailEvent::BeforeWrite,
                session,
                &effect.path,
            )
            .await?;
            if answer != RailAnswer::Proceed {
                continue;
            }
            write_effect(&self.cwd, effect).map_err(|e| e.to_string())?;
            if self.formats {
                ask_hook(
                    self.dialect,
                    &self.rail,
                    RailEvent::AfterWrite,
                    session,
                    &effect.path,
                )
                .await?;
            }
        }
        Ok(())
    }
}

/// Asks the engine about one moment of a session, as its CLI asks: the stdin
/// the CLI's hook sends, decoded by the dialect's codec, answered by the
/// engine, and encoded into the reply the hook leaves. The session decides on
/// what the CLI reads back from that reply, so an encoding the CLI would
/// misread changes this session as it changes the real one. A call with
/// nothing to ask goes on without asking.
pub(super) async fn ask_hook(
    dialect: DialectName,
    rail: &RailEndpoint,
    event: RailEvent,
    session: &SessionId,
    path: &Path,
) -> std::result::Result<RailAnswer, String> {
    let codec = dialect
        .rail_codec()
        .ok_or_else(|| "the CLI has no hook codec".to_string())?;
    let stdin = dialect.hook_stdin(event, session, path);
    let call = codec
        .decode(event, &stdin)
        .map_err(|e| format!("the {event} hook call did not read: {e}"))?;
    let Some(call) = call else {
        return Ok(RailAnswer::Proceed);
    };
    let rail = rail.clone();
    let answer = tokio::task::spawn_blocking(move || rail.ask(&call, Some(HOOK_DEADLINE)))
        .await
        .map_err(|e| format!("the {event} hook did not finish: {e}"))?
        .map_err(|e| format!("the {event} hook could not ask the engine: {e}"))?;
    let reply = codec.encode(event, &answer);
    let read = dialect.read_reply(event, &reply);
    tracing::debug!(
        exit = reply.exit,
        engine = ?answer,
        cli = ?read,
        "a mock hook answered as its CLI reads it"
    );
    Ok(read)
}

impl MockAdapter {
    /// Whether an effect lands. A command effect lands by the confinement's
    /// judgement of its write by directory, as a filesystem sandbox judges it.
    /// A tool effect under a filesystem fence lands by the same judgement of
    /// its path: that sandbox knows nothing of globs, so no scope is judged
    /// here. A fence judged per tool call, or none at all, lets a tool effect
    /// land, and the engine's rail or its post-check judges what follows.
    fn lands(&self, req: &SessionRequest, effect: &MockEffect) -> bool {
        match effect.via {
            Via::Command => req
                .confinement
                .command_may_write(&req.cwd, &req.cwd.join(&effect.path)),
            Via::Tool => match self.capabilities.fence {
                FenceLevel::None | FenceLevel::ToolCalls => true,
                FenceLevel::Filesystem => matches!(
                    Fence::everything(req.fence.advice).judge(
                        &req.cwd,
                        &effect.path,
                        &req.confinement
                    ),
                    Verdict::Allowed
                ),
            },
        }
    }

    /// The effects a session that asks the engine per write is handed: every
    /// tool effect, which the rail judges as it is asked, and each command
    /// effect that lands, since no hook sees a command's write.
    pub(super) fn landing(&self, req: &SessionRequest, effects: &[MockEffect]) -> Vec<MockEffect> {
        effects
            .iter()
            .filter(|effect| effect.via == Via::Tool || self.lands(req, effect))
            .cloned()
            .collect()
    }

    /// Applies one session's effects under the request's `cwd`, leaving out
    /// the ones that do not land: a write the fence or the confinement refuses
    /// is blocked before it lands. A refused write is reported by nothing, as
    /// a CLI's own sandbox reports nothing.
    pub(super) fn apply_effects(&self, script: &SessionScript, req: &SessionRequest) -> Result<()> {
        for effect in &script.effects {
            if !self.lands(req, effect) {
                continue;
            }
            write_effect(&req.cwd, effect)?;
        }
        Ok(())
    }
}

/// Writes one scripted effect under `cwd`, creating what it lies under.
pub(super) fn write_effect(cwd: &Path, effect: &MockEffect) -> Result<()> {
    let full_path = cwd.join(&effect.path);
    let io_err = |action: String, source: std::io::Error| AdapterError::AdapterIo {
        adapter: ID.clone(),
        action,
        source,
    };
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| io_err(format!("create directories for {}", full_path.display()), e))?;
    }
    std::fs::write(&full_path, &effect.content)
        .map_err(|e| io_err(format!("write {}", full_path.display()), e))
}
