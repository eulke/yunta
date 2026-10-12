//! `adapters.claude-code.adapter_settings`: the CLI-specific settings this
//! adapter reads. `sandbox` is the mode the `Edit` profile's commands run
//! under — `workspace-write` unless a team narrows it to `read-only` or
//! widens it to `danger-full-access`, the same modes and the same key as
//! the `codex` adapter's. Any other key is an unknown setting, reported by
//! `probe()`.

use serde::Deserialize;
use yunta_core::{AdapterSettings, Result};

use crate::session::{typed_settings, Sandbox};

/// The keys this adapter reads. A key outside them is an unknown setting.
pub(crate) const KNOWN: &[&str] = &["sandbox"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClaudeSettings {
    #[serde(default)]
    pub sandbox: Option<Sandbox>,
}

impl ClaudeSettings {
    pub(super) fn read(settings: &AdapterSettings) -> Result<Self> {
        typed_settings(&super::ID, settings.adapter_settings.as_ref(), KNOWN)
    }
}
