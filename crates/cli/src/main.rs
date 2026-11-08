//! `yunta` binary entrypoint. Stays thin by design: parse argv with clap, then
//! hand off to `cli`, which routes each subcommand to its module under
//! `commands/`. Its map:
//!
//! - Commands: `cli` parses and routes, `commands` holds one module per
//!   subcommand, `context` opens each one, `help` prints the help, `graph`
//!   draws a workflow's graph, `pack` installs and lists packs, `project`
//!   resolves the project and its config, and `detect` reads what a repository
//!   says about itself.
//! - What a person sees and answers: `output` owns what a command writes,
//!   the streams it is drawn to and the code it exits with, `surface` draws a
//!   run as it happens, `run_page` draws where a
//!   run stands, `ask` and `human_interaction` take a person's answers, and
//!   `identity` names who decided.
//! - For machines and the process: `json` is the versioned JSON the CLI emits,
//!   `error` is the one error a command hands back, and `interrupt` is what
//!   trips cancellation.

// A panic is a bug, never a fallible path: production returns a typed
// error instead of unwrapping, expecting, indexing, or panicking.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::indexing_slicing
)]
// Tests are the one place a failed assertion is meant to abort. The panic
// family is lifted there by `clippy.toml`; `indexing_slicing` has no such
// switch, so it is lifted in test builds here. Integration tests are
// separate crates neither reaches.
#![cfg_attr(test, allow(clippy::indexing_slicing))]

mod ask;
mod cli;
mod commands;
mod context;
mod detect;
mod error;
mod graph;
mod help;
mod human_interaction;
mod identity;
mod interrupt;
mod json;
mod output;
mod pack;
mod project;
mod run_page;
mod surface;

use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use clap::{FromArgMatches, ValueEnum};
use surco::paint::ColorWhen;
use yunta_core::Env;

use crate::error::CliError;

use crate::output::Outcome;

fn main() -> ExitCode {
    let started = Instant::now();

    // The process environment, read once here at the shell's edge: every
    // color, glyph, width, terminal and log decision below reads this value.
    let env = project::process_env();
    // The arguments, read once. The help and the usage errors clap refuses
    // are painted before clap parses them, each rendered by a command built
    // in the paint of the stream it goes to; `help::exit` writes what that
    // command rendered.
    let args: Vec<OsString> = std::env::args_os().collect();
    let policy = policy_from_args(&env, &args);
    let stdout_paint = policy.paint(std::io::stdout().is_terminal());
    let stderr_paint = policy.paint(std::io::stderr().is_terminal());
    let matches =
        help::parse(&args, stdout_paint, stderr_paint).unwrap_or_else(|error| help::exit(&error));
    let cli = cli::Cli::from_arg_matches(&matches).unwrap_or_else(|error| help::exit(&error));
    init_tracing(&env);
    output::settle_color(
        color_policy(&env, cli.color),
        set_var(&env, surco::paint::LOOK_VAR),
    );
    output::settle_columns(columns(&env));
    output::settle_glyphs(&glyph_env(&env));
    surface::terminal::settle(&env);
    // Read once the settling above is done, so every line this process
    // writes to stdout takes the same color, width and glyphs.
    let look = output::stdout_look();
    let mode = cli.mode();
    tracing::debug!("starting");

    let outcome = drive(cli, started, mode, &look);
    // Last, because a prompt this run abandoned mid-key left a thread
    // that is still reading the terminal and still turning raw mode on
    // between its own reads. Nothing runs after this, so nothing takes
    // the terminal back off the shell this process returns to.
    ask::restore_terminal();

    exit_status(outcome, mode, &look)
}

/// The status of every failure, reported or refused: the one place the CLI spells it.
const FAILED: ExitCode = ExitCode::FAILURE;

/// The status a command's outcome leaves the process with. A refused error
/// is written to the reader first, in the mode and look the command ran with.
fn exit_status(
    outcome: Result<Outcome, CliError>,
    mode: output::Mode,
    look: &surco::look::Look,
) -> ExitCode {
    match outcome {
        Ok(outcome) => ExitCode::from(outcome.code()),
        Err(error) => {
            output::refused(&error, mode, look);
            FAILED
        }
    }
}

/// Installs the process's log subscriber, filtered by `RUST_LOG` as `env`
/// holds it. Unset or unreadable, the filter keeps errors only, as
/// `EnvFilter::from_default_env` does.
fn init_tracing(env: &Env) {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing_subscriber::filter::LevelFilter::ERROR.into())
        .parse_lossy(env.rust_log.as_deref().unwrap_or_default());
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// The terminal variable `name` as this process was started with it, when
/// it is set and not empty: an empty variable reads as unset.
fn set_var<'a>(env: &'a Env, name: &str) -> Option<&'a str> {
    env.terminal_var(name).filter(|value| !value.is_empty())
}

/// What decides whether a stream gets color: the command line, and the
/// variables the convention reads, as this process was started with them.
fn color_policy(env: &Env, when: surco::paint::ColorWhen) -> surco::paint::ColorPolicy {
    surco::paint::ColorPolicy {
        when,
        no_color: set_var(env, "NO_COLOR").is_some(),
        force: set_var(env, "CLICOLOR_FORCE").is_some_and(|value| value != "0"),
        off: set_var(env, "CLICOLOR").is_some_and(|value| value == "0"),
        dumb: set_var(env, "TERM").is_some_and(|value| value == "dumb"),
        links: links(env),
        look: surco::paint::look_of(set_var(env, surco::paint::LOOK_VAR)),
        truecolor: set_var(env, "COLORTERM")
            .is_some_and(|value| matches!(value, "truecolor" | "24bit")),
        ground: surco::paint::ground_of(set_var(env, "COLORFGBG")),
    }
}

/// The policy for `env` and the `--color` `args` name. The help is painted
/// before clap parses the arguments, so the value is read from them here.
fn policy_from_args(env: &Env, args: &[OsString]) -> surco::paint::ColorPolicy {
    let words = args
        .iter()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned());
    color_policy(env, color_flag(words))
}

/// The `--color` value among `args`, in the forms clap reads it: `--color
/// VALUE` or `--color=VALUE`, before a `--` that ends the options. A repeated
/// `--color` reads as its first value, the one clap takes. A `--color` after a
/// `--help` still paints the help: clap answers the help before it reaches
/// that flag, but the reader asked for it. A value clap would refuse reads as
/// the default, `auto`, and clap refuses it when it parses the arguments. Every
/// other line takes the value clap parsed; this reading serves only the help
/// and the refusals, which are painted before clap parses.
fn color_flag(args: impl IntoIterator<Item = String>) -> ColorWhen {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let value = match arg.as_str() {
            "--" => break,
            "--color" => args.next(),
            other => other.strip_prefix("--color=").map(str::to_string),
        };
        if let Some(value) = value {
            return ColorWhen::from_str(&value, false).unwrap_or_default();
        }
    }
    ColorWhen::default()
}

/// Whether a path is a link on a stream with color: what
/// `YUNTA_HYPERLINKS` says outright, or what the terminal announced about
/// itself.
fn links(env: &Env) -> surco::paint::Links {
    use surco::paint::Links;
    match set_var(env, "YUNTA_HYPERLINKS") {
        Some("1") => return Links::Forced,
        Some("0") => return Links::Refused,
        _ => {}
    }
    let program = set_var(env, "TERM_PROGRAM");
    let vte = set_var(env, "VTE_VERSION").and_then(|version| version.parse::<u32>().ok());
    let announced = matches!(program, Some("iTerm.app" | "WezTerm" | "vscode"))
        || vte.is_some_and(|version| version >= 5000)
        || set_var(env, "KITTY_WINDOW_ID").is_some()
        || set_var(env, "WT_SESSION").is_some();
    match announced {
        true => Links::Announced,
        false => Links::Unknown,
    }
}

/// The three values the glyph policy reads, as this process was started
/// with them.
fn glyph_env(env: &Env) -> surco::glyphs::GlyphEnv {
    surco::glyphs::GlyphEnv {
        explicit: env
            .terminal_var(surco::glyphs::OVERRIDE_VAR)
            .map(str::to_string),
        locale: ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|name| set_var(env, name).map(str::to_string)),
        term: env.terminal_var("TERM").map(str::to_string),
    }
}

/// The width a reader asked every line to be laid out in, when
/// `COLUMNS` names one.
fn columns(env: &Env) -> Option<usize> {
    env.terminal_var("COLUMNS")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|cells| *cells > 0)
}

/// Runs `cli` on a runtime of this process's own, and leaves.
///
/// The runtime is built here rather than by `#[tokio::main]` for what
/// happens after the command returns: dropping a runtime blocks until
/// every blocking task has finished, and one of them is a terminal read
/// waiting on a key. A run stopped from outside stops waiting on that
/// key — the prompt hands the engine its answer, the engine kills the
/// run's tree and writes the run's close, and the command returns — and
/// the read is then a thread parked on a keystroke nobody is going to
/// press. Waiting on it would keep a process alive that has nothing
/// left to do, so this hands the runtime back without waiting and the
/// process exits.
///
/// Everything the run owns is already released by then: what this
/// leaves behind is one thread inside a `read`, and the exit takes it.
///
/// `started` is the first instant of this process, handed on to the
/// command so a hook's window counts from the start of the process.
/// `mode` and `look` are how this process writes, decided once by `main`
/// and handed to whichever command runs.
fn drive(
    cli: cli::Cli,
    started: Instant,
    mode: output::Mode,
    look: &surco::look::Look,
) -> Result<Outcome, CliError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|source| CliError::io("build the runtime for", "this command", source))?;
    let outcome = runtime.block_on(cli.run(started, mode, look));
    runtime.shutdown_background();
    outcome
}

/// The run manifest at `path`, read through the one door every
/// persisted document goes through — so a manifest from a newer binary
/// is refused naming both versions, and a key this one does not know
/// comes back on the document instead of being dropped.
pub(crate) fn load_manifest(
    path: &Path,
) -> Result<yunta_core::persisted::PersistedDoc<yunta_core::Manifest>, CliError> {
    let bytes = std::fs::read(path)
        .map_err(|source| CliError::io("read run manifest at", path.display(), source))?;
    yunta_core::persisted::PersistedDoc::read(&bytes)
        .map_err(|error| CliError::msg(yunta_core::describe(&error)))
}

/// The workflow at `path`, read through the one door that holds it to
/// its own rules — never the bare parser, which would hand back a graph
/// nobody checked.
pub(crate) fn load_workflow(path: &Path) -> Result<yunta_core::Workflow, CliError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|source| CliError::io("read workflow at", path.display(), source))?;
    yunta_core::workflow::read::read(&contents, path).map_err(|report| CliError::Document {
        report,
        text: Some(contents.clone()),
    })
}

/// The workflow at `path` read in one pass: every problem the file has,
/// and the workflow it declares once the keys nothing reads are taken
/// out, when what is left reads. `yunta check` judges that workflow too,
/// so a person fixes every problem in one round; nothing runs it.
pub(crate) fn audit_workflow(
    path: &Path,
) -> Result<
    (
        Option<yunta_core::Workflow>,
        yunta_core::diagnostic::Report,
        String,
    ),
    CliError,
> {
    let contents = std::fs::read_to_string(path)
        .map_err(|source| CliError::io("read workflow at", path.display(), source))?;
    let (workflow, report) = yunta_core::workflow::read::read_all(&contents, path);
    Ok((workflow, report, contents))
}

/// Reads and parses a YAML file into `T`, naming what it was reading and
/// where when it can't — the loader for everything that is neither a
/// workflow nor a persisted document.
pub(crate) fn load_yaml<T: serde::de::DeserializeOwned>(
    path: &Path,
    what: &str,
) -> Result<T, CliError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|source| CliError::io(&format!("read {what} at"), path.display(), source))?;
    yunta_core::yaml::parse(&contents)
        .map_err(|e| CliError::msg(format!("failed to parse {what} at {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use surco::paint::ColorWhen;

    use super::color_flag;

    /// The `--color` value the words name, as the arguments after the
    /// program's own name.
    fn named(words: &[&str]) -> ColorWhen {
        color_flag(words.iter().map(|word| word.to_string()))
    }

    /// Both spellings of `--color` are read wherever they stand among the
    /// arguments, before the options end.
    #[test]
    fn color_is_read_in_both_forms_wherever_it_stands() {
        assert_eq!(named(&["--color", "never"]), ColorWhen::Never);
        assert_eq!(named(&["--color=always"]), ColorWhen::Always);
        assert_eq!(
            named(&["run", "wf.yaml", "--color", "never"]),
            ColorWhen::Never
        );
    }

    /// A repeated `--color` reads as its first value, which clap takes, and a
    /// `--` ends the options before any `--color` after it.
    #[test]
    fn the_first_color_before_a_double_dash_decides() {
        assert_eq!(
            named(&["--color", "always", "--color=never"]),
            ColorWhen::Always
        );
        assert_eq!(
            named(&["--color", "never", "--", "--color", "always"]),
            ColorWhen::Never
        );
        assert_eq!(named(&["--", "--color", "always"]), ColorWhen::Auto);
    }

    /// A `--help` between two `--color`s is answered before the second one is
    /// read, so the help is painted with the first.
    #[test]
    fn a_help_between_two_colors_is_painted_with_the_first() {
        assert_eq!(
            named(&["--color", "never", "--help", "--color", "always"]),
            ColorWhen::Never
        );
    }

    /// A `--color` after a `--help` decides the help's paint, though clap
    /// answers the help before reading it.
    #[test]
    fn a_color_after_a_help_still_paints_it() {
        assert_eq!(named(&["--help", "--color", "always"]), ColorWhen::Always);
    }

    /// A value clap refuses reads as `auto`, the default.
    #[test]
    fn a_value_clap_refuses_reads_as_auto() {
        assert_eq!(named(&["--color", "sometimes"]), ColorWhen::Auto);
        assert_eq!(named(&["--color"]), ColorWhen::Auto);
    }
}
