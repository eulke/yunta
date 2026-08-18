use thiserror::Error;

/// Typed errors shared across the workspace's public APIs (CLAUDE.md:
/// "errores tipados en la lib, contexto en el borde" — never `unwrap()`
/// or `expect()` outside tests).
#[derive(Debug, Error)]
pub enum YuntaError {
    /// An adapter was asked for a capability it never declared (Spec
    /// Adapter §4/§7, A2/A6): the engine fails typed instead of
    /// emulating or degrading in silence.
    #[error("adapter `{adapter}` does not support `{what}`")]
    Unsupported { adapter: String, what: &'static str },
}

/// Convenience alias for the workspace's typed `Result`.
pub type Result<T> = std::result::Result<T, YuntaError>;
