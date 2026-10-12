//! What a scripted session writes, and which of it its fence keeps out.

use std::path::PathBuf;

use yunta_core::fence::{Fence, Verdict};
use yunta_core::port::SessionRequest;
use yunta_core::{AdapterError, FenceLevel, Result};

use super::fixture::{MockEffect, SessionScript};
use super::{MockAdapter, ID};

impl MockAdapter {
    /// Whether the fence refuses an effect at `path` — by the same
    /// judge every adapter asks, so a fixture exercises the rule the
    /// real ones enforce.
    ///
    /// A filesystem sandbox knows nothing of globs: at that level the
    /// judgement is against the directories alone. A fixture with no
    /// fence at all writes whatever it scripts, and the engine's own
    /// post-check diff is what catches it.
    fn refuses(&self, req: &SessionRequest, path: &std::path::Path) -> bool {
        let fence = match self.fixture.capabilities.fence {
            FenceLevel::None => return false,
            FenceLevel::ToolCalls => req.fence.clone(),
            FenceLevel::Filesystem => Fence::everything(req.fence.roots.clone(), req.fence.advice),
        };
        matches!(fence.judge(&req.cwd, path), Verdict::Refused(_))
    }

    /// Applies one session's filesystem effects under the request's
    /// `cwd`, leaving out the ones the fence refuses: an adapter that
    /// can judge a write blocks it before it lands. What the session
    /// reports about a blocked write is `refused`'s business.
    pub(super) fn apply_effects(&self, script: &SessionScript, req: &SessionRequest) -> Result<()> {
        for effect in &script.effects {
            if self.refuses(req, &effect.path) {
                continue;
            }
            write_effect(&req.cwd, effect)?;
        }
        Ok(())
    }

    /// The effects the fence kept from landing that the session reports
    /// as writes it refused. Only a judgement per tool call reports: a
    /// filesystem sandbox leaves its denied writes out without a word, as
    /// a CLI's own sandbox does.
    pub(super) fn refused(&self, script: &SessionScript, req: &SessionRequest) -> Vec<PathBuf> {
        match self.fixture.capabilities.fence {
            FenceLevel::ToolCalls => script
                .effects
                .iter()
                .filter(|e| self.refuses(req, &e.path))
                .map(|e| e.path.clone())
                .collect(),
            FenceLevel::None | FenceLevel::Filesystem => Vec::new(),
        }
    }
}

/// Writes one scripted effect under `cwd`, creating what it lies under.
pub(super) fn write_effect(cwd: &std::path::Path, effect: &MockEffect) -> Result<()> {
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
