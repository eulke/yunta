//! The words of a command that ran and of a red check: the line that names
//! what ran and how it ended, and the lines it printed last.

use yunta_core::events::{CommandExit, CommandOrigin, RedCheck};
use yunta_core::text;

/// The line that names a command and how it ended, without what it printed.
pub fn command_headline(exit: &CommandExit) -> String {
    match &exit.origin {
        None => format!("exit {}", exit.code),
        Some(CommandOrigin::Executor { executor }) => {
            format!("executor `{executor}` exited {}", exit.code)
        }
        Some(CommandOrigin::Hook { phase, command }) => {
            format!("{} hook `{command}` failed", phase.as_str())
        }
    }
}

/// The headline and, under it, the lines the command printed last. A
/// command that failed saying nothing — `test -f x` is the ordinary case —
/// reads as its headline alone.
pub fn command_exit(exit: &CommandExit) -> String {
    text::detailed(command_headline(exit), &exit.tail.join("\n"))
}

/// The sentence of a red check: its headline, and under it what the check
/// printed last: the line a command concluded with, or the tail of a suite
/// that regressed.
pub(super) fn red_sentence(red: &RedCheck) -> String {
    match red {
        RedCheck::Command { conclusion, .. } => {
            text::detailed(red_headline(red), conclusion.as_deref().unwrap_or_default())
        }
        RedCheck::Regression { tail, .. } => text::detailed(red_headline(red), &tail.join("\n")),
        RedCheck::Coverage { .. } => red_headline(red),
    }
}

/// The line a red check states: what it measured or how it exited.
pub(super) fn red_headline(red: &RedCheck) -> String {
    match red {
        RedCheck::Coverage {
            measured: Some(measured),
            threshold,
            ..
        } => format!("coverage {measured}% is below the {threshold}% threshold"),
        RedCheck::Coverage {
            cmd,
            measured: None,
            ..
        } => format!("`{cmd}` produced no `NN[.NN]%` coverage figure in its stdout"),
        RedCheck::Command { cmd, exit_code, .. } => format!("`{cmd}` exits {exit_code}"),
        RedCheck::Regression { cmd, exit_code, .. } => {
            format!("regression: `{cmd}` passed at baseline (exit 0) but now exits {exit_code}")
        }
    }
}
