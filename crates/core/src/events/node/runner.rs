//! Why a runner candidate was not used, and the discarded candidate that
//! carries the reason. A candidate is passed over either because the run
//! pinned another adapter or because its adapter is not available here.

use serde::{Deserialize, Serialize};

use crate::config::RunnerCandidate;
use crate::events::LoggedWords;

/// Why a runner candidate was not used. A log written before the typed
/// reason stored the sentence, which reads back as `Recorded { sentence }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerRejection {
    /// The run's `--adapter` flag pinned another adapter.
    NotPinned { pinned: crate::ids::AdapterId },
    /// The candidate's adapter is not available here.
    Unavailable,
    /// A sentence an older log stored, kept as written. The field is
    /// named: a newtype variant of `String` does not serialize under an
    /// internal tag.
    Recorded { sentence: LoggedWords },
}

/// A candidate the resolution passed over before the one it chose, and
/// why. A candidate after the chosen one was never tried, and is not one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiscardedCandidate {
    pub candidate: RunnerCandidate,
    #[serde(deserialize_with = "reason_or_sentence")]
    pub reason: RunnerRejection,
}

/// Reads the typed reason, or the sentence a log written before it stored.
fn reason_or_sentence<'de, D>(deserializer: D) -> Result<RunnerRejection, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Typed(RunnerRejection),
        Sentence(String),
    }
    Ok(match Stored::deserialize(deserializer)? {
        Stored::Typed(reason) => reason,
        Stored::Sentence(sentence) => RunnerRejection::Recorded {
            sentence: LoggedWords::read(sentence),
        },
    })
}
