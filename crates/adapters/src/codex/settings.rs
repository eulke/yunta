//! `adapters.codex.adapter_settings`: the CLI-specific settings this
//! adapter reads. `sandbox` is the `codex exec --sandbox` mode the
//! `Edit` profile runs under — `workspace-write` unless a team narrows
//! it to `read-only` or widens it to `danger-full-access`; `ReadOnly`
//! and `Full` map to their own modes regardless. Any other key is an
//! unknown setting, reported by `probe()`.

use serde::Deserialize;
use yunta_core::{AdapterSettings, Result};

use crate::session::typed_settings;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CodexSettings {
    #[serde(default)]
    pub sandbox: Option<Sandbox>,
}

/// `codex exec --sandbox` modes, as the CLI spells them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Sandbox {
    ReadOnly,
    #[default]
    WorkspaceWrite,
    DangerFullAccess,
}

impl Sandbox {
    pub(super) fn as_flag(self) -> &'static str {
        match self {
            Sandbox::ReadOnly => "read-only",
            Sandbox::WorkspaceWrite => "workspace-write",
            Sandbox::DangerFullAccess => "danger-full-access",
        }
    }
}

impl CodexSettings {
    pub(super) const KNOWN: &'static [&'static str] = &["sandbox"];

    pub(super) fn read(settings: &AdapterSettings) -> Result<Self> {
        typed_settings(&super::ID, settings.adapter_settings.as_ref(), Self::KNOWN)
    }
}
