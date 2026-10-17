//! What a session's CLI said when it failed, as data: a limit its account
//! reached, or a refusal and the reason it gave.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::AdapterId;

/// What a session's failure says about the request. An adapter reads it
/// from its CLI's own fields and words, and the engine turns it into a
/// failure: a limit becomes [`Failure::Limit`](super::Failure::Limit), a
/// refusal becomes [`Failure::SessionRefused`](super::Failure::SessionRefused).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureCause {
    /// The account's limit was reached; becomes `Failure::Limit`, never a refusal.
    /// `None` when the CLI named no reset time.
    Limit { resets_at: Option<DateTime<Utc>> },
    /// The session refused the request, and why.
    Refused(RefusalCause),
}

/// Why a session refused the request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RefusalCause {
    /// Unclassified: final for this attempt, never re-attempted on its own.
    Final,
    /// The credential the session used was refused.
    Authentication,
    /// The model the session asked for is not one the account can use.
    InvalidModel,
    /// Transport markers only; the one cause the engine re-attempts by itself.
    Unreachable,
    /// The CLI could not keep the session's conversation within the model's
    /// context window. This session cannot go on, though its work can: the
    /// engine hands it to one fresh session, which carries none of that
    /// conversation.
    ContextExhausted,
}

/// A refusal kept whole: the adapter's message and its cause.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionRefusal {
    pub adapter: AdapterId,
    pub cause: RefusalCause,
    pub message: String,
}
