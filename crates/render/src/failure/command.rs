//! The words of a command that ran, of a red check, and of a command the
//! project's permissions refused: the line that names what ran and how it
//! ended, the lines it printed last, and what refused it.

use yunta_core::events::{
    CommandExit, CommandOrigin, DeniedBy, DeniedCommand, DeniedRule, RedCheck,
};
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

/// The words of a command the project's permissions refused: the command,
/// and the list that refused it. A refusal an older log stored reads as the
/// sentence it stored.
pub fn denied(rule: &DeniedRule) -> String {
    match rule {
        DeniedRule::Command(DeniedCommand { command, by }) => match by {
            DeniedBy::Deny { pattern } => format!(
                "command `{command}` matches denied pattern `{pattern}` (permissions.commands.deny)"
            ),
            DeniedBy::Allow => format!(
                "command `{command}` matches no pattern in the strict allowlist (permissions.commands.allow)"
            ),
        },
        DeniedRule::Logged(said) => said.as_str().to_string(),
    }
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

#[cfg(test)]
mod tests {
    use yunta_core::events::{DeniedBy, DeniedCommand, DeniedRule};

    use super::denied;

    #[test]
    fn a_command_the_deny_list_refuses_says_the_pattern_that_matched() {
        let rule = DeniedRule::Command(DeniedCommand {
            command: "echo forbidden-marker".into(),
            by: DeniedBy::Deny {
                pattern: "*forbidden-marker*".into(),
            },
        });
        assert_eq!(
            denied(&rule),
            "command `echo forbidden-marker` matches denied pattern `*forbidden-marker*` \
             (permissions.commands.deny)"
        );
    }

    #[test]
    fn a_command_the_allowlist_leaves_out_says_no_pattern_of_it_matched() {
        let rule = DeniedRule::Command(DeniedCommand {
            command: "rm -rf x".into(),
            by: DeniedBy::Allow,
        });
        assert_eq!(
            denied(&rule),
            "command `rm -rf x` matches no pattern in the strict allowlist \
             (permissions.commands.allow)"
        );
    }

    #[test]
    fn a_refusal_an_older_log_stored_reads_as_it_was_written() {
        let sentence =
            "command `rm -rf x` matches denied pattern `rm *` (permissions.commands.deny)";
        assert_eq!(denied(&DeniedRule::Logged(sentence.into())), sentence);
    }
}
