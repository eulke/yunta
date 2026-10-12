//! What a scripted session writes, which of it its fence keeps out, and
//! how the writes its CLI judges one by one are asked of the engine.

use std::path::{Path, PathBuf};
use std::time::Duration;

use yunta_core::fence::{Fence, Verdict};
use yunta_core::port::SessionRequest;
use yunta_core::rail::{RailAnswer, RailEndpoint, RailEvent};
use yunta_core::{AdapterError, FenceLevel, Result, SessionId};

use super::fixture::{MockEffect, SessionScript};
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
    /// Asks the engine about each effect and writes the ones it allows.
    pub(super) async fn written(&self, session: &SessionId) -> std::result::Result<(), String> {
        for effect in &self.effects {
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
    /// Whether the fence refuses an effect at `path` — by the same
    /// judge every adapter asks, so a fixture exercises the rule the
    /// real ones enforce.
    ///
    /// A filesystem sandbox knows nothing of globs: at that level the
    /// judgement is against the directories alone. A fixture with no
    /// fence at all writes whatever it scripts, and the engine's own
    /// post-check diff is what catches it. A fence judged per tool call
    /// is not judged here: its writes are asked of the engine's rail.
    fn refuses(&self, req: &SessionRequest, path: &Path) -> bool {
        let fence = match self.capabilities.fence {
            FenceLevel::None | FenceLevel::ToolCalls => return false,
            FenceLevel::Filesystem => Fence::everything(req.fence.roots.clone(), req.fence.advice),
        };
        matches!(fence.judge(&req.cwd, path), Verdict::Refused(_))
    }

    /// Applies one session's filesystem effects under the request's
    /// `cwd`, leaving out the ones the fence refuses: an adapter that
    /// can judge a write blocks it before it lands. A refused write is
    /// reported by nothing, as a CLI's own sandbox reports nothing.
    pub(super) fn apply_effects(&self, script: &SessionScript, req: &SessionRequest) -> Result<()> {
        for effect in &script.effects {
            if self.refuses(req, &effect.path) {
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
