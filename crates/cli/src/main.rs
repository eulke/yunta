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
//!   `surface` and `render` draw a run as it happens, `run_page` draws where a
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
mod render;
mod run_page;
mod surface;

use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use clap::{CommandFactory, FromArgMatches};
use yunta_core::Env;

use crate::error::{CliError, Outcome};

fn main() -> ExitCode {
    let started = Instant::now();

    let command = cli::Cli::command();
    let template = help::template(&command);
    let matches = command.help_template(template).get_matches();
    let cli = cli::Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    // The process environment, read once here at the shell's edge: every
    // color, glyph, width, terminal and log decision below reads this value.
    let env = project::process_env();
    init_tracing(&env);
    render::settle_color(color_policy(&env, cli.color));
    render::settle_columns(columns(&env));
    render::settle_glyphs(&glyph_env(&env));
    surface::terminal::settle(&env);
    // Read once the settling above is done, so every line this process
    // writes to stdout takes the same color, width and glyphs.
    let look = render::stdout_look();
    let mode = cli.mode();
    tracing::debug!("yunta starting");

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
    look: &render::Look,
) -> ExitCode {
    match outcome {
        Ok(Outcome::Success) => ExitCode::SUCCESS,
        Ok(Outcome::Code(code)) => ExitCode::from(code),
        Ok(Outcome::Reported) => FAILED,
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
fn color_policy(env: &Env, when: render::ink::ColorWhen) -> render::ink::ColorPolicy {
    render::ink::ColorPolicy {
        when,
        no_color: set_var(env, "NO_COLOR").is_some(),
        force: set_var(env, "CLICOLOR_FORCE").is_some_and(|value| value != "0"),
        off: set_var(env, "CLICOLOR").is_some_and(|value| value == "0"),
        dumb: set_var(env, "TERM").is_some_and(|value| value == "dumb"),
        links: links(env),
    }
}

/// Whether a path is a link on a stream with color: what
/// `YUNTA_HYPERLINKS` says outright, or what the terminal announced about
/// itself.
fn links(env: &Env) -> render::ink::Links {
    use render::ink::Links;
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
fn glyph_env(env: &Env) -> render::glyphs::GlyphEnv {
    render::glyphs::GlyphEnv {
        explicit: env
            .terminal_var(render::glyphs::OVERRIDE_VAR)
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
    look: &render::Look,
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
