//! `adapters.codex.adapter_settings`: the CLI-specific settings this
//! adapter reads. `sandbox` is the mode the `Edit` profile runs under —
//! `workspace-write` unless a team narrows it to `read-only` or widens it
//! to `danger-full-access`; `ReadOnly` and `Full` map to their own modes
//! regardless. Any other key is an unknown setting, reported by
//! `probe()`.

use serde::Deserialize;
use yunta_core::{AdapterSettings, Result};

use crate::session::{typed_settings, Sandbox};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CodexSettings {
    #[serde(default)]
    pub sandbox: Option<Sandbox>,
}

impl CodexSettings {
    pub(super) const KNOWN: &'static [&'static str] = &["sandbox"];

    pub(super) fn read(settings: &AdapterSettings) -> Result<Self> {
        typed_settings(&super::ID, settings.adapter_settings.as_ref(), Self::KNOWN)
    }
}
