//! What a scripted session writes, which of it its fence or its confinement
//! keeps out, and how the writes its CLI judges one by one are asked of the
//! engine.

use std::path::{Path, PathBuf};
use std::time::Instant;

use yunta_core::fence::{Fence, Verdict};
use yunta_core::port::SessionRequest;
use yunta_core::rail::answer::{FormatAnswer, HookAnswer, WriteAnswer};
use yunta_core::rail::hook::{settle, RailFailure};
use yunta_core::rail::window::{AskBudget, Wait};
use yunta_core::rail::{RailEndpoint, RailEvent};
use yunta_core::{AdapterError, FenceLevel, Result, SessionId};

use super::fixture::{MockEffect, SessionScript, Via};
use super::{MockAdapter, ID};
use crate::dialect::DialectName;

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
    /// A refused write is skipped: the engine's refusal is the answer, not a
    /// failed session. A write made through a shell command reaches no write
    /// hook, so the command effects handed here, which have landed, are
    /// written as they are made, with nothing asked and nothing reported.
    pub(super) async fn written(&self, session: &SessionId) -> std::result::Result<(), String> {
        for effect in &self.effects {
            if effect.via == Via::Command {
                write_effect(&self.cwd, effect).map_err(|e| e.to_string())?;
                continue;
            }
            let answer: WriteAnswer =
                ask_hook(self.dialect, &self.rail, session, &effect.path).await;
            if answer != WriteAnswer::Proceed {
                continue;
            }
            write_effect(&self.cwd, effect).map_err(|e| e.to_string())?;
            if self.formats {
                ask_hook::<FormatAnswer>(self.dialect, &self.rail, session, &effect.path).await;
            }
        }
        Ok(())
    }
}

/// Asks the engine about one moment of a session, as its CLI's hook asks: the
/// stdin the CLI sends is decoded by the dialect's codec, and the engine's
/// answer is settled as the real hook settles it, within the moment's window.
/// A write's answer is due within the write window and a stop's within the
/// judgement. A call that cannot be asked gets the moment's unasked answer,
/// so no moment fails the session.
pub(super) async fn ask_hook<A: HookAnswer + Send + 'static>(
    dialect: DialectName,
    rail: &RailEndpoint,
    session: &SessionId,
    path: &Path,
) -> A {
    let call = match dialect.rail_codec() {
        Some(codec) => {
            let stdin = dialect.hook_stdin(A::MOMENT, session, path);
            codec
                .decode(A::MOMENT, &stdin)
                .map_err(|e| RailFailure::Undecodable(e.to_string()))
        }
        None => Err(RailFailure::Undecodable(
            "the CLI has no hook codec".to_string(),
        )),
    };
    let wait = match A::MOMENT {
        RailEvent::BeforeStop => Wait::Judgement,
        RailEvent::BeforeWrite | RailEvent::AfterWrite => {
            Wait::Write(AskBudget::from_process_start(Instant::now()))
        }
    };
    let rail = rail.clone();
    tokio::task::spawn_blocking(move || settle::<A>(Ok(&rail), call, wait))
        .await
        .unwrap_or_else(|e| A::unasked(&format!("the {} hook did not finish: {e}", A::MOMENT)))
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
