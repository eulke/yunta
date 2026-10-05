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

    /// An adapter cannot build the fence this session needs. A
    /// capability that is absent fails rather than opening a session
    /// that writes where the run never said it could.
    #[error("adapter `{adapter}`: {source}")]
    FenceUnbuildable {
        adapter: AdapterId,
        #[source]
        source: Unbuildable,
    },

    /// An adapter was handed `adapter_settings` it cannot read. No
    /// session opens under them: a setting nobody could parse would run
    /// the agent under something the config never asked for, silently.
    #[error("adapter `{adapter}` cannot read its `adapter_settings`: {detail}")]
    UnreadableSettings { adapter: AdapterId, detail: String },

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

/// Why an adapter cannot build a session's fence.
#[derive(Debug, thiserror::Error)]
pub enum Unbuildable {
    #[error("this adapter cannot run its fence hook: no yunta binary was handed to the session")]
    HookUnavailable,
    #[error(
        "this adapter cannot keep {} writable under a read-only profile; hand the files over \
         through the run tools or raise the profile to `edit`",
        .0.iter().map(|root| root.display().to_string()).collect::<Vec<_>>().join(", ")
    )]
    SealedRoots(Vec<std::path::PathBuf>),
}

/// Convenience alias for an adapter's typed `Result`.
pub type Result<T> = std::result::Result<T, AdapterError>;

/// The error and every cause behind it, joined for a person to read —
/// what an edge prints, so no cause is lost when a typed error is
/// rendered once.
pub fn describe(error: &dyn std::error::Error) -> String {
    with_causes(error.to_string(), error)
}

/// `said`, then each cause behind `error` that it does not already state.
///
/// An error that names its cause in its own sentence and also hands it on
/// as its source would otherwise say it twice; a sentence an edge wrote
/// for an error, in place of the error's own, still carries what caused it.
pub fn with_causes(said: String, error: &dyn std::error::Error) -> String {
    let mut text = said;
    let mut cause = error.source();
    while let Some(next) = cause {
        let stated = next.to_string();
        if !stated.is_empty() && !text.contains(&stated) {
            text.push_str(": ");
            text.push_str(&stated);
        }
        cause = next.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An error that names its cause in its sentence and hands it on.
    #[derive(Debug, thiserror::Error)]
    #[error("cannot read the plan: {source}")]
    struct Embeds {
        #[source]
        source: std::io::Error,
    }

    /// An error that leaves its cause to whoever reads the chain.
    #[derive(Debug, thiserror::Error)]
    #[error("the run cannot go on")]
    struct Hands {
        #[source]
        source: Embeds,
    }

    fn missing() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no such file")
    }

    #[test]
    fn a_cause_the_sentence_already_states_is_not_said_again() {
        let error = Embeds { source: missing() };
        assert_eq!(describe(&error), "cannot read the plan: no such file");
    }

    #[test]
    fn every_cause_behind_an_error_is_said_once_in_order() {
        let error = Hands {
            source: Embeds { source: missing() },
        };
        assert_eq!(
            describe(&error),
            "the run cannot go on: cannot read the plan: no such file"
        );
        assert_eq!(
            with_causes("not resumed".to_string(), &error),
            "not resumed: cannot read the plan: no such file"
        );
    }
}
