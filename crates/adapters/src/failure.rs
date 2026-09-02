//! What a CLI's failure message says about retrying. The message is
//! the only signal the CLIs give, so the reading is by marker: the
//! phrases the CLIs and the APIs behind them print for a refused
//! credential or an unknown model. Anything else is a failure of the
//! attempt, and a fresh session may do better.

/// Why a session failed, as far as its message tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The credential was refused; another attempt with the same one
    /// fails the same way.
    Authentication,
    /// The model named is not one this account can use; another
    /// attempt asks for the same model.
    InvalidModel,
    /// Anything else: a transport error, a rate limit, a turn that
    /// ended badly.
    Other,
}

impl FailureKind {
    /// Whether a fresh session with the same request can do better.
    pub fn retryable(self) -> bool {
        matches!(self, FailureKind::Other)
    }
}

const AUTHENTICATION_MARKERS: &[&str] = &[
    "invalid api key",
    "authentication",
    "unauthorized",
    "not logged in",
    "401",
];

/// Read together with the word `model`, which on its own says nothing
/// about the failure.
const MODEL_MARKERS: &[&str] = &[
    "not found",
    "does not exist",
    "invalid",
    "unsupported",
    "unknown",
    "404",
];

pub fn classify(message: &str) -> FailureKind {
    let text = message.to_lowercase().replace('_', " ");
    if AUTHENTICATION_MARKERS
        .iter()
        .any(|marker| text.contains(marker))
    {
        FailureKind::Authentication
    } else if text.contains("model") && MODEL_MARKERS.iter().any(|marker| text.contains(marker)) {
        FailureKind::InvalidModel
    } else {
        FailureKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_credential_is_never_retried() {
        for message in [
            "Invalid API key · Please run /login",
            "401 Unauthorized: invalid API key",
            "authentication_error: x-api-key header is required",
            "Not logged in",
        ] {
            assert_eq!(classify(message), FailureKind::Authentication, "{message}");
            assert!(!classify(message).retryable());
        }
    }

    #[test]
    fn an_unknown_model_is_never_retried() {
        for message in [
            "API Error: 404 {\"type\":\"not_found_error\",\"message\":\"model: claude-nope\"}",
            "The model `gpt-nope` does not exist",
            "unsupported model: o1-preview",
        ] {
            assert_eq!(classify(message), FailureKind::InvalidModel, "{message}");
        }
    }

    #[test]
    fn everything_else_is_an_attempt_that_may_do_better_next_time() {
        for message in [
            "rate limited",
            "model response stream ended unexpectedly",
            "stream disconnected before completion",
            "file not found: src/main.rs",
        ] {
            assert_eq!(classify(message), FailureKind::Other, "{message}");
            assert!(classify(message).retryable());
        }
    }
}
