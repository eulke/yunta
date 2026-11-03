//! The help `yunta` prints for itself: its commands grouped by what a
//! person is doing with them, in the order they meet it, each with its
//! one-line summary, and the first steps as examples. It also reads the
//! arguments the way clap does, with each output in the paint of its own
//! stream: the help on stdout, and a usage error on stderr. A paint reaches
//! the page through the styles clap draws its own lines in and through the
//! template written here, so a command built in one paint renders in that
//! paint alone.

use std::ffi::OsString;

use clap::{ArgMatches, Command, CommandFactory};
use surco::line::Tone;
use surco::paint::Paint;
use surco::width::{GAP, INDENT};

use crate::commands::advice;

/// What a person is doing, in the order they meet it, and the commands
/// that do it. Every visible subcommand is in exactly one group.
pub(crate) const GROUPS: [(&str, &[&str]); 6] = [
    ("Start", &["init", "doctor"]),
    ("Write workflows", &["new", "check", "schema", "graph"]),
    ("Run", &["run", "resume", "test"]),
    (
        "Follow and decide",
        &["status", "open", "list", "resolve-gate", "cancel", "close"],
    ),
    ("Audit", &["verify", "receipt", "stats"]),
    ("Extend and maintain", &["pack", "mcp", "gc", "completions"]),
];

/// The first steps, each with what it does.
fn examples() -> [(String, &'static str); 4] {
    [
        (advice::init(), "prepare this repository"),
        (
            advice::run(".yunta/workflows/lint-fix.yaml"),
            "run a workflow from the catalog",
        ),
        (
            advice::list_runs(),
            "this repository's runs, what needs you first",
        ),
        (
            advice::status("needs"),
            "the run waiting on you, and how to answer it",
        ),
    ]
}

/// The command clap reads the arguments with, its help and its usage lines
/// painted in `paint`, the paint of the stream they are written to. The
/// styles clap draws its own lines in and the help template both come from
/// `paint`, so a command built in one paint renders in that paint alone.
fn command(paint: Paint) -> Command {
    let base = crate::cli::Cli::command();
    let template = template(&base, paint);
    base.styles(surco::paint::clap_styles(paint))
        .help_template(template)
}

/// The help template for the root `command`: its summary and usage, its
/// subcommands under their groups, its options, and the examples, each
/// heading in the tone clap draws a heading in and what a reader types
/// strong. The template's own lines are painted in `paint`, the paint of
/// the stream the help is printed on. clap draws the usage line and the
/// options itself, in the styles `surco::paint::clap_styles` gives the same
/// paint.
fn template(command: &Command, paint: Paint) -> String {
    let heading = |title: &str| paint.word(Tone::Muted, &format!("{title}:"));
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
            out.push_str(&row(name, column, &about, paint));
        }
    }
    out.push_str(&format!(
        "\n{}\n{{options}}\n\n{}\n",
        heading("Options"),
        heading("Examples")
    ));
    let steps = examples();
    let column = steps
        .iter()
        .map(|(example, _)| example.len())
        .max()
        .unwrap_or(0);
    for (example, gloss) in steps {
        out.push_str(&row(&example, column, gloss, paint));
    }
    out
}

/// What a reader types, painted strong in `paint`, in a column `column`
/// wide; what it does beside it stays plain.
fn row(typed: &str, column: usize, does: &str, paint: Paint) -> String {
    let pad = " ".repeat(column.saturating_sub(typed.len()));
    format!(
        "{INDENT}{}{pad}{GAP}{does}\n",
        paint.word(Tone::Strong, typed)
    )
}

/// What clap makes of `args`: the arguments it reads, the help or the
/// version it answers with, or the usage error it refuses with. A help is
/// painted in `stdout`, the paint of the stream it goes to, and a usage
/// error in `stderr`, the paint of its own stream: each is rendered by a
/// command built in that paint, through its styles and its template. The
/// two may differ, since the policy decides each stream on its own.
pub(crate) fn parse(
    args: &[OsString],
    stdout: Paint,
    stderr: Paint,
) -> Result<ArgMatches, clap::Error> {
    command(stdout)
        .try_get_matches_from(args.to_vec())
        .map_err(|error| match error.use_stderr() {
            true => refusal(args, stderr).unwrap_or(error),
            false => error,
        })
}

/// The usage error clap makes of `args`, with its usage lines and messages
/// painted in `paint`. A command's styles are fixed when it is built, so the
/// arguments are read again by a command built in that paint. `None` only if
/// they read after all, which arguments clap has refused cannot do.
fn refusal(args: &[OsString], paint: Paint) -> Option<clap::Error> {
    command(paint).try_get_matches_from(args.to_vec()).err()
}

/// Writes what clap made of the arguments, through the one module that writes
/// the CLI's output, and exits with clap's own status for it: a usage error on
/// stderr, the help and the version on stdout. The error carries the styles of
/// the command that made it, built in the paint of that stream, so it writes
/// what that paint asks for, and no escape where the paint is plain.
pub(crate) fn exit(error: &clap::Error) -> ! {
    // best-effort: a closed pipe gets no more; the exit status still says what happened.
    let _ = crate::output::clap_answer(error);
    std::process::exit(error.exit_code())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

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

    /// The arguments `words` make, as the process reads them.
    fn argv(words: &[&str]) -> Vec<OsString> {
        words.iter().copied().map(OsString::from).collect()
    }

    /// What `parse` answers `words` with, as clap renders it, when `stdout`
    /// and `stderr` are the paints of their streams.
    fn rendered(words: &[&str], stdout: Paint, stderr: Paint) -> String {
        parse(&argv(words), stdout, stderr)
            .expect_err("the arguments are refused or answered")
            .render()
            .ansi()
            .to_string()
    }

    /// A usage error is painted by stderr's paint alone and a help by
    /// stdout's alone, so a piped stdout with a terminal stderr gives each
    /// its own paint. A painted text differs from the plain one where its
    /// stream is painted, and not where the other stream is.
    #[test]
    fn a_refusal_and_a_help_take_the_paint_of_their_own_stream() {
        let plain = Paint::Plain;
        let refused = ["yunta", "--no-such-flag"];
        let asked = ["yunta", "--help"];
        assert_eq!(
            rendered(&refused, plain, plain),
            rendered(&refused, Paint::ANSI16, plain)
        );
        assert_ne!(
            rendered(&refused, plain, plain),
            rendered(&refused, plain, Paint::ANSI16)
        );
        assert_eq!(
            rendered(&asked, plain, Paint::ANSI16),
            rendered(&asked, plain, plain)
        );
        assert_ne!(
            rendered(&asked, Paint::ANSI16, plain),
            rendered(&asked, plain, plain)
        );
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

    /// Each example the help gives is a command line clap reads, so a renamed
    /// command cannot leave one behind.
    #[test]
    fn every_example_is_a_command_line_yunta_reads() {
        for (example, _) in examples() {
            assert!(
                crate::cli::Cli::try_parse_from(example.split_whitespace()).is_ok(),
                "`{example}` is not a command line yunta reads"
            );
        }
    }
}
