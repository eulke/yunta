//! How a scripted session ends, and what it does when it is stopped: the
//! outcome a fixture closes a session with, the refusals it may state, and
//! the answers it gives to an interrupt and a kill.

use serde::Deserialize;
use yunta_core::events::{FailureCause, RefusalCause};

use super::FixtureError;
use crate::dialect::DialectName;

/// The refusals a fixture may state, as a person writes them. A fixture
/// states what its message says, never a retry: whether a session is
/// re-attempted is the engine's to decide from the refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclaredCause {
    Final,
    Unreachable,
}

impl From<DeclaredCause> for FailureCause {
    fn from(declared: DeclaredCause) -> Self {
        match declared {
            DeclaredCause::Final => FailureCause::Refused(RefusalCause::Final),
            DeclaredCause::Unreachable => FailureCause::Refused(RefusalCause::Unreachable),
        }
    }
}

/// How the session's stream ends. `Crash` and `Hang` exist to exercise
/// the engine's side of detecting a session that ends with no terminal
/// event, and of interrupt/kill — a mock terminal `Completed`/`Failed`
/// on its own can't test either.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MockOutcome {
    Completed {
        summary: String,
    },
    Failed {
        message: String,
        /// The cause the message must read as under the fixture's dialect,
        /// when the fixture states one. A fixture whose message reads
        /// otherwise is refused when it is read.
        #[serde(default)]
        cause: Option<DeclaredCause>,
    },
    /// The account's limit ends the session, as its CLI reports one.
    /// `resets_at` is the reset the CLI names, in Unix seconds as its
    /// stream carries it; a CLI that never prints one takes none.
    Limited {
        #[serde(default)]
        resets_at: Option<i64>,
    },
    /// The stream ends with no terminal event at all — a real crash.
    Crash,
    /// The stream never produces another item on its own; `kill` ends
    /// it unless refused, and `interrupt` ends it or not as `on_interrupt`
    /// says (still with no terminal event, as a forced kill leaves a real session).
    Hang {
        #[serde(default)]
        on_interrupt: OnInterrupt,
    },
}

impl MockOutcome {
    /// Refuses a declared cause that the message does not read as under
    /// `dialect`, the CLI the fixture stands for, and a limit's reset that
    /// CLI never prints.
    pub(crate) fn check(&self, dialect: DialectName) -> Result<(), FixtureError> {
        if let MockOutcome::Limited { resets_at: Some(_) } = self {
            if !dialect.prints_a_reset() {
                return Err(FixtureError::UnprintedReset);
            }
        }
        let MockOutcome::Failed {
            message,
            cause: Some(declared),
        } = self
        else {
            return Ok(());
        };
        let declared = FailureCause::from(*declared);
        let actual = dialect.failure_cause(message);
        if declared == actual {
            return Ok(());
        }
        Err(FixtureError::Cause {
            declared: word(declared),
            actual: word(actual),
        })
    }
}

/// The word a fixture writes a cause as.
fn word(cause: FailureCause) -> &'static str {
    match cause {
        FailureCause::Limit { .. } => "limit",
        FailureCause::Refused(RefusalCause::Final) => "final",
        FailureCause::Refused(RefusalCause::Authentication) => "authentication",
        FailureCause::Refused(RefusalCause::InvalidModel) => "invalid_model",
        FailureCause::Refused(RefusalCause::Unreachable) => "unreachable",
        FailureCause::Refused(RefusalCause::ContextExhausted) => "context_exhausted",
    }
}

/// What a hung session does with an ordered stop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnInterrupt {
    /// Ends, as a CLI that honors SIGINT does.
    #[default]
    End,
    /// Keeps hanging; only `kill` ends it — the case the engine's
    /// interrupt-then-kill escalation exists for.
    Ignore,
}

/// Scripted answer to `kill` while the script still plays: `Refuse` fails it
/// and leaves the session running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KillBehavior {
    Refuse,
}
