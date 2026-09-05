use thiserror::Error;

use crate::ids::AdapterId;
use crate::Capability;

/// What an adapter can fail with — the one error type every adapter
/// speaks, so the engine handles a failure by its kind and the edge
/// renders it once.
#[derive(Debug, Error)]
pub enum AdapterError {
    /// An adapter was asked for a capability it never declared: the
    /// engine fails typed instead of emulating or degrading in silence.
    #[error("adapter `{adapter}` does not support `{what}`")]
    Unsupported {
        adapter: AdapterId,
        what: Capability,
    },

    /// An adapter operation failed at the I/O boundary — e.g. `mock`
    /// applying a fixture's filesystem effects, or a real adapter
    /// failing to spawn its CLI subprocess.
    #[error("adapter `{adapter}` failed to {action}")]
    AdapterIo {
        adapter: AdapterId,
        action: String,
        #[source]
        source: std::io::Error,
    },

    /// An adapter refused an operation for a reason of its own that is
    /// not an OS error — e.g. `mock` asked to spawn more sessions than
    /// its fixture scripts. The message says what to fix.
    #[error("adapter `{adapter}`: {message}")]
    Adapter { adapter: AdapterId, message: String },

    /// `adapters.<id>.adapter_settings` names a key the adapter does not
    /// read — a typo, or a setting of another adapter.
    #[error(
        "adapter `{adapter}`: unknown setting `{key}` in `adapter_settings` — it reads {}",
        if known.is_empty() { "no settings at all".to_string() } else { format!("only: {}", known.join(", ")) }
    )]
    UnknownSetting {
        adapter: AdapterId,
        key: String,
        known: Vec<&'static str>,
    },
}

/// Convenience alias for an adapter's typed `Result`.
pub type Result<T> = std::result::Result<T, AdapterError>;

/// The error and every cause behind it, joined for a person to read —
/// what an edge prints, so no cause is lost when a typed error is
/// rendered once.
pub fn describe(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(next) = cause {
        text.push_str(": ");
        text.push_str(&next.to_string());
        cause = next.source();
    }
    text
}
