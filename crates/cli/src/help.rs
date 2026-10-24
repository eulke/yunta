//! The help `yunta` prints for itself: its commands grouped by what a
//! person is doing with them, in the order they meet it, each with its
//! one-line summary, and the first steps as examples.

use clap::Command;
use surco::line::Tone;
use surco::paint::Ink;
use surco::width::{GAP, INDENT};

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
    ("Extend and maintain", &["pack", "mcp", "gc", "completions"]),
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

/// The help template for `command`, in the words and tones every
/// subcommand's help has: its summary and usage, its subcommands under
/// their groups, its options, and the examples, each heading in the tone
/// clap draws a heading in and what a reader types strong. clap strips the
/// paint where the help is not drawn in color.
pub(crate) fn template(command: &Command) -> String {
    let heading = |title: &str| Ink::Ansi16.word(Tone::Muted, &format!("{title}:"));
    let column = GROUPS
        .iter()
        .flat_map(|(_, names)| names.iter())
        .map(|name| name.len())
        .max()
        .unwrap_or(0);
    let mut out = String::from("{about-with-newline}\n{usage-heading} {usage}\n");
    for (group, names) in GROUPS {
        out.push_str(&format!("\n{}\n", heading(group)));
        for name in names {
            let about = command
                .find_subcommand(name)
                .and_then(Command::get_about)
                .map(ToString::to_string)
                .unwrap_or_default();
            out.push_str(&row(name, column, &about));
        }
    }
    out.push_str(&format!(
        "\n{}\n{{options}}\n\n{}\n",
        heading("Options"),
        heading("Examples")
    ));
    let column = EXAMPLES
        .iter()
        .map(|(example, _)| example.len())
        .max()
        .unwrap_or(0);
    for (example, gloss) in EXAMPLES {
        out.push_str(&row(example, column, gloss));
    }
    out
}

/// What a reader types, strong, in a column `column` wide, and what it
/// does beside it.
fn row(typed: &str, column: usize, does: &str) -> String {
    let pad = " ".repeat(column.saturating_sub(typed.len()));
    format!(
        "{INDENT}{}{pad}{GAP}{does}\n",
        Ink::Ansi16.word(Tone::Strong, typed)
    )
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
