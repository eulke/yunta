//! What every adapter in this crate shares: reading its own settings,
//! handing a prompt to a CLI that already runs, and naming what a tool
//! acted on without repeating the thing itself.
//!
//! The port these adapters implement is `yunta_core::port`.

use yunta_core::{AdapterError, AdapterId, Result};

/// Reads an adapter's `adapter_settings` map into its typed settings:
/// every key must be one of `known`, and the typed struct reads the
/// values. The one place a setting name is checked, so an unknown key
/// is always the same error whichever adapter it reaches.
pub fn typed_settings<T: serde::de::DeserializeOwned>(
    adapter: &'static AdapterId,
    raw: Option<&serde_json::Map<String, serde_json::Value>>,
    known: &'static [&'static str],
) -> Result<T> {
    let raw = raw.cloned().unwrap_or_default();
    if let Some(key) = raw.keys().find(|key| !known.contains(&key.as_str())) {
        return Err(AdapterError::UnknownSetting {
            adapter: adapter.clone(),
            key: key.clone(),
            known: known.to_vec(),
        });
    }
    serde_json::from_value(serde_json::Value::Object(raw)).map_err(|e| AdapterError::Adapter {
        adapter: adapter.clone(),
        message: format!("`adapter_settings`: {e}"),
    })
}
