//! What a CLI's failure message says about the refusal. The message is
//! the only signal the CLIs give, so the reading is by marker: the
//! phrases the CLIs and the APIs behind them print for a refused
//! credential, an unknown model, or a service the machine cannot reach.
//! A message none of those names is a final refusal of its session.

use yunta_core::events::{FailureCause, RefusalCause};

/// Whether `marker` occurs in `text` as whole words: neither edge of the
/// occurrence touches a letter or a digit, so `401` is not part of `4010`.
pub(crate) fn has_word(text: &str, marker: &str) -> bool {
    text.match_indices(marker).any(|(at, _)| {
        !text[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
            && !text[at + marker.len()..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
    })
}

/// What the CLIs, and the network libraries under them, print when the
/// service cannot be reached: the machine cannot connect to it, or the
/// stream that carries its answer breaks before the answer completes.
/// Both are transport failures, and the only refusal the engine re-attempts
/// by itself.
const TRANSPORT_MARKERS: &[&str] = &[
    "enotfound",
    "eai_again",
    "econnrefused",
    "enetunreach",
    "etimedout",
    "can't reach the api server",
    "stream disconnected before completion",
    "stream ended unexpectedly",
];

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

/// The refusal a CLI's failure message names, by marker. A message no class
/// names is a final refusal: its session is not re-attempted on its own.
pub fn classify(message: &str) -> FailureCause {
    let lower = message.to_lowercase();
    let text = lower.replace('_', " ");
    let hit =
        |haystack: &str, markers: &[&str]| markers.iter().any(|marker| has_word(haystack, marker));
    // The transport markers read the lowercased message with its underscores
    // kept, so `EAI_AGAIN` is `eai_again`.
    let refusal = if hit(&lower, TRANSPORT_MARKERS) {
        RefusalCause::Unreachable
    } else if hit(&text, AUTHENTICATION_MARKERS) {
        RefusalCause::Authentication
    } else if has_word(&text, "model") && hit(&text, MODEL_MARKERS) {
        RefusalCause::InvalidModel
    } else {
        RefusalCause::Final
    };
    FailureCause::Refused(refusal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_match_whole_words() {
        for message in [
            "connect to port 4010 failed",
            "the model invalidates the cache",
            "file not found: src/main.rs",
        ] {
            assert_eq!(
                classify(message),
                FailureCause::Refused(RefusalCause::Final),
                "{message}"
            );
        }
        assert_eq!(
            classify("connect ECONNREFUSED 127.0.0.1:8080"),
            FailureCause::Refused(RefusalCause::Unreachable)
        );
        // Codex's limit phrases are read by its parser only.
        assert_eq!(
            classify("You've hit your usage limit. Try again later."),
            FailureCause::Refused(RefusalCause::Final)
        );
    }

    #[test]
    fn a_refused_credential_is_never_retried() {
        for message in [
            "Invalid API key · Please run /login",
            "401 Unauthorized: invalid API key",
            "authentication_error: x-api-key header is required",
            "Not logged in",
        ] {
            assert_eq!(
                classify(message),
                FailureCause::Refused(RefusalCause::Authentication),
                "{message}"
            );
        }
    }

    #[test]
    fn an_unknown_model_is_never_retried() {
        for message in [
            "API Error: 404 {\"type\":\"not_found_error\",\"message\":\"model: claude-nope\"}",
            "The model `gpt-nope` does not exist",
            "unsupported model: o1-preview",
        ] {
            assert_eq!(
                classify(message),
                FailureCause::Refused(RefusalCause::InvalidModel),
                "{message}"
            );
        }
    }

    #[test]
    fn connectivity_messages_classify_unreachable() {
        for message in [
            "API Error: Can't reach the API server — check your internet or DNS (ENOTFOUND)",
            "getaddrinfo EAI_AGAIN api.anthropic.com",
            "connect ECONNREFUSED 127.0.0.1:8080",
            "connect ENETUNREACH 2606:4700::6810:1",
            "request to https://api.openai.com/v1/responses failed: ETIMEDOUT",
            "stream disconnected before completion",
            "model response stream ended unexpectedly",
        ] {
            assert_eq!(
                classify(message),
                FailureCause::Refused(RefusalCause::Unreachable),
                "{message}"
            );
        }
    }
}
