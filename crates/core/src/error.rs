use thiserror::Error;

/// Typed errors shared across the workspace's public APIs (CLAUDE.md:
/// "errores tipados en la lib, contexto en el borde" — never `unwrap()`
/// or `expect()` outside tests).
#[derive(Debug, Error)]
pub enum YuntaError {
    /// An adapter was asked for a capability it never declared: the
    /// engine fails typed instead of emulating or degrading in silence.
    #[error("adapter `{adapter}` does not support `{what}`")]
    Unsupported { adapter: String, what: &'static str },

    /// An adapter operation failed at the I/O boundary — e.g. `mock`
    /// applying a fixture's filesystem effects, or a real adapter
    /// failing to spawn its CLI subprocess.
    #[error("adapter `{adapter}` failed to {action}")]
    AdapterIo {
        adapter: String,
        action: String,
        #[source]
        source: std::io::Error,
    },

    /// An adapter refused an operation for a reason of its own that is
    /// not an OS error — e.g. `mock` asked to spawn more sessions than
    /// its fixture scripts. The message says what to fix.
    #[error("adapter `{adapter}`: {message}")]
    Adapter { adapter: String, message: String },
}

/// Convenience alias for the workspace's typed `Result`.
pub type Result<T> = std::result::Result<T, YuntaError>;
