//! The help `yunta` prints for itself: its commands grouped by what a
//! person is doing with them, in the order they meet it, each with its
//! one-line summary, and the first steps as examples.

use clap::Command;

/// What a person is doing, in the order they meet it, and the commands
/// that do it. Every visible subcommand is in exactly one group.
pub(crate) const GROUPS: [(&str, &[&str]); 6] = [
    ("Start", &["init", "doctor"]),
    ("Write workflows", &["new", "check", "schema", "graph"]),
    ("Run", &["run", "resume", "test"]),
    (
        "Follow and decide",
        &["status", "list", "resolve-gate", "cancel", "close"],
    ),
    ("Audit", &["verify", "receipt", "stats"]),
    ("Extend and maintain", &["pack", "mcp", "gc"]),
];

/// The first steps, each with what it does.
const EXAMPLES: [(&str, &str); 4] = [
    ("yunta init", "prepare this repository"),
    (
        "yunta run .yunta/workflows/lint-fix.yaml",
        "run a workflow from the catalog",
    ),
    (
        "yunta list --runs",
        "this repository's runs, what needs you first",
    ),
    (
        "yunta status needs",
        "the run waiting on you, and how to answer it",
    ),
];

/// The help template for `command`: its summary and usage, its
/// subcommands under their groups, its options, and the examples.
pub(crate) fn template(command: &Command) -> String {
    let column = GROUPS
        .iter()
        .flat_map(|(_, names)| names.iter())
        .map(|name| name.len())
        .max()
        .unwrap_or(0);
    let mut out = String::from("{about-with-newline}\n{usage-heading} {usage}\n");
    for (group, names) in GROUPS {
        out.push_str(&format!("\n{group}:\n"));
        for name in names {
            let about = command
                .find_subcommand(name)
                .and_then(Command::get_about)
                .map(ToString::to_string)
                .unwrap_or_default();
            out.push_str(&format!("  {name:<column$}  {about}\n"));
        }
    }
    out.push_str("\nOptions:\n{options}\n\nExamples:\n");
    let width = EXAMPLES
        .iter()
        .map(|(example, _)| example.len())
        .max()
        .unwrap_or(0);
    for (example, gloss) in EXAMPLES {
        out.push_str(&format!("  {example:<width$}  {gloss}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    /// The subcommands a person sees: every one clap lists, less the
    /// hidden ones and its own `help`.
    fn visible() -> Vec<String> {
        crate::cli::Cli::command()
            .get_subcommands()
            .filter(|command| !command.is_hide_set() && command.get_name() != "help")
            .map(|command| command.get_name().to_string())
            .collect()
    }

    #[test]
    fn every_visible_subcommand_is_in_exactly_one_help_group() {
        let grouped: Vec<&str> = GROUPS
            .iter()
            .flat_map(|(_, names)| names.iter().copied())
            .collect();
        for name in visible() {
            assert_eq!(
                grouped.iter().filter(|listed| **listed == name).count(),
                1,
                "`{name}` is in {} groups",
                grouped.iter().filter(|listed| **listed == name).count()
            );
        }
        for name in grouped {
            assert!(
                visible().iter().any(|command| command == name),
                "`{name}` is grouped and is no subcommand"
            );
        }
    }

    #[test]
    fn every_summary_is_at_most_fifty_characters() {
        let command = crate::cli::Cli::command();
        for name in visible() {
            let about = command
                .find_subcommand(&name)
                .and_then(Command::get_about)
                .map(ToString::to_string)
                .unwrap_or_default();
            assert!(
                !about.is_empty() && about.chars().count() <= 50,
                "`{name}`: {about:?}"
            );
        }
    }
}
