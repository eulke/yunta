//! `adapters.claude-code.adapter_settings`: the CLI-specific settings
//! this adapter reads. Every portable concept — model, agent,
//! permissions, budget — is a typed field of the session request, and
//! nothing about the `claude` CLI needs a setting beyond those, so the
//! set is empty: any key is an unknown setting, reported by `probe()`.

use serde::Deserialize;
use yunta_core::{AdapterSettings, Result};

use crate::session::typed_settings;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClaudeSettings {}

impl ClaudeSettings {
    pub(super) const KNOWN: &'static [&'static str] = &[];

    pub(super) fn read(settings: &AdapterSettings) -> Result<Self> {
        typed_settings(
            "claude-code",
            settings.adapter_settings.as_ref(),
            Self::KNOWN,
        )
    }
}
