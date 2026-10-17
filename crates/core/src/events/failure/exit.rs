//! How a process a node depended on ended, what an account's limit said,
//! and what a command that never answered means: a session's CLI that died
//! without a terminal event, or a command that exited non-zero, each with
//! the last lines it printed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::events::HookPhase;
use crate::hash::ContentHash;
use crate::ids::AdapterId;

/// How many of the last lines a process printed a failure keeps: enough
/// to read a compiler's error or a CLI's startup error, and not enough
/// for a whole build log to ride along in an event and into every
/// surface that quotes the log.
pub const TAIL_LINES: usize = 20;

/// How the process ended: the status it exited with, or the signal that
/// ended it.
///
/// A closed union rather than two optionals, so a process that says
/// neither is not representable. A stored `type` this build does not
/// know reads back as [`SessionEnd::Unknown`] — the tolerance everything
/// persisted here gives a reader older than its writer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEnd {
    Code {
        code: i32,
    },
    Signal {
        signal: i32,
    },
    #[serde(other)]
    Unknown,
}

/// What a process left behind: how it ended, and the last lines it wrote
/// to stderr, redacted of every declared secret and session token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionExit {
    pub end: SessionEnd,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stderr_tail: Vec<String>,
}

/// What a command that exited non-zero left behind: whose command it was
/// when it was not the node's own `run:`, the code it exited with, the
/// last lines it printed — stdout, then stderr, as a reader reads them —
/// and the run object holding everything it printed.
///
/// The tail is what a person deciding about the failure reads first, so
/// it rides on the event; the whole output stays in the run's object
/// store, where a reader who needs more than the tail finds it by hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CommandExit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<CommandOrigin>,
    pub code: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tail: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<ContentHash>,
}

/// Who ran a command that is not the node's own `run:`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandOrigin {
    /// An external executor the node delegated its work to.
    Executor { executor: String },
    /// One of the node's hooks, as the workflow wrote its command.
    Hook { phase: HookPhase, command: String },
}

/// A session that ended without ever reporting a terminal event: whose
/// it was, and how its process went, when it had one of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionDeath {
    pub adapter: AdapterId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<SessionExit>,
}

/// The account's limit, as the session or the forge reported it: where it
/// came from, when it resets if it said so, and the words it came with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AccountLimit {
    pub source: LimitSource,
    /// Absolute: the moment the limit lifts, when the source named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<DateTime<Utc>>,
    pub message: String,
}

/// Which limit stopped the run: the account of a session's CLI, or the
/// forge's rate limit on the run's pull requests.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LimitSource {
    /// The account of the session's CLI.
    Session { adapter: AdapterId },
    /// The forge's rate limit.
    Forge,
}

/// Why a command gave no answer of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Unanswered {
    /// The shell found no command of that name.
    NotFound,
    /// The shell found the command but could not run it.
    NotExecutable,
    /// The engine stopped the command before it answered.
    Stopped,
}

/// The exit code the engine records for a command it stopped before it
/// answered. Negative, so no code a shell reports equals it; a command that
/// a signal ended already reads as -1, so the engine's own stop takes the next.
pub const STOPPED_EXIT_CODE: i32 = -2;

impl Unanswered {
    /// What `exit_code` says when the command never answered; `None` when
    /// it is the command's own.
    pub fn of(exit_code: i32) -> Option<Self> {
        match exit_code {
            // The shell's own codes for a command it could not find, and for one it could not run.
            127 => Some(Unanswered::NotFound),
            126 => Some(Unanswered::NotExecutable),
            STOPPED_EXIT_CODE => Some(Unanswered::Stopped),
            _ => None,
        }
    }

    /// The words a command that never answered is called by.
    pub fn words(self) -> &'static str {
        match self {
            Unanswered::NotFound => "command not found",
            Unanswered::NotExecutable => "command not executable",
            Unanswered::Stopped => "stopped before it answered",
        }
    }

    /// An exit code as a reader reads it: the code, and what it means when
    /// the command never answered.
    pub fn exit_described(exit_code: i32) -> String {
        match Unanswered::of(exit_code) {
            Some(why) => format!("{exit_code} ({})", why.words()),
            None => exit_code.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_that_never_answered_is_called_by_its_words() {
        assert_eq!(Unanswered::NotFound.words(), "command not found");
        assert_eq!(Unanswered::NotExecutable.words(), "command not executable");
        assert_eq!(Unanswered::Stopped.words(), "stopped before it answered");
    }

    #[test]
    fn an_exit_code_says_what_it_means_when_the_command_never_answered() {
        assert_eq!(Unanswered::exit_described(127), "127 (command not found)");
        assert_eq!(
            Unanswered::exit_described(126),
            "126 (command not executable)"
        );
        assert_eq!(
            Unanswered::exit_described(STOPPED_EXIT_CODE),
            "-2 (stopped before it answered)"
        );
    }

    #[test]
    fn an_exit_code_of_the_command_itself_is_written_as_the_code() {
        assert_eq!(Unanswered::exit_described(1), "1");
        assert_eq!(Unanswered::exit_described(0), "0");
    }
}
