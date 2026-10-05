//! How a process a node depended on ended: a session's CLI that died
//! without a terminal event, or a command that exited non-zero, each with
//! the last lines it printed.

use std::fmt;

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
/// to stderr, redacted of every value its environment carried.
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

impl CommandExit {
    /// The line that names the command and how it ended, without what it
    /// printed.
    pub fn headline(&self) -> String {
        match &self.origin {
            None => format!("exit {}", self.code),
            Some(CommandOrigin::Executor { executor }) => {
                format!("executor `{executor}` exited {}", self.code)
            }
            Some(CommandOrigin::Hook { phase, command }) => {
                format!("{} hook `{command}` failed", phase.as_str())
            }
        }
    }
}

impl fmt::Display for CommandExit {
    /// The headline and, under it, the lines the command printed last. A
    /// command that failed saying nothing — `test -f x` is the ordinary
    /// case — reads as its headline alone.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&crate::text::detailed(
            self.headline(),
            &self.tail.join("\n"),
        ))
    }
}

/// A session that ended without ever reporting a terminal event: whose
/// it was, and how its process went, when it had one of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionDeath {
    pub adapter: AdapterId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<SessionExit>,
}

impl fmt::Display for SessionDeath {
    /// Every shape the type admits, the absence included: a session with
    /// no process of its own has nothing to report about one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(exit) = &self.exit else {
            return write!(
                f,
                "session `{}` ended without a terminal event",
                self.adapter
            );
        };
        write!(
            f,
            "session `{}` {} before any terminal event",
            self.adapter, exit.end
        )?;
        match exit.stderr_tail.last() {
            Some(last) => write!(f, " — {last}"),
            None => Ok(()),
        }
    }
}

impl fmt::Display for SessionEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionEnd::Code { code } => write!(f, "exited with code {code}"),
            SessionEnd::Signal { signal } => write!(f, "was killed by signal {signal}"),
            SessionEnd::Unknown => f.write_str("ended in a way this build does not know"),
        }
    }
}
