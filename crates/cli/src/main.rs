//! `yunta` binary entrypoint. Stays thin by design: parse argv with clap,
//! then hand off to `cli`, which routes each subcommand to its module
//! under `commands/`.

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
mod pack;
mod project;
mod render;
mod surface;

use std::path::Path;
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches};

use crate::error::{CliError, Outcome};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let command = cli::Cli::command();
    let template = help::template(&command);
    let matches = command.help_template(template).get_matches();
    let cli = cli::Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    render::settle_color(color_policy(cli.color));
    render::settle_columns(columns());
    render::settle_glyphs(&glyph_env());
    surface::terminal::settle();
    tracing::debug!("yunta starting");

    let outcome = drive(cli);
    // Last, because a prompt this run abandoned mid-key left a thread
    // that is still reading the terminal and still turning raw mode on
    // between its own reads. Nothing runs after this, so nothing takes
    // the terminal back off the shell this process returns to.
    ask::restore_terminal();

    match outcome {
        Ok(Outcome::Success) => ExitCode::SUCCESS,
        Ok(Outcome::Reported) => ExitCode::FAILURE,
        Ok(Outcome::Code(code)) => ExitCode::from(code),
        Err(error) => {
            refused(&error);
            ExitCode::FAILURE
        }
    }
}

/// A refusal on stderr: the sentence after `error:`, then where it
/// looked and the commands that go on from here, when it has them.
fn refused(error: &CliError) {
    let ink = crate::render::stderr_ink();
    eprintln!(
        "{}: {}",
        ink.word(render::ink::Tone::Failed, "error"),
        error.said_to(crate::error::Reader::Person)
    );
    let (detail, next) = error.advice();
    if let Some(detail) = detail {
        eprintln!(
            "{}{}",
            render::INDENT,
            ink.word(render::ink::Tone::Muted, &detail)
        );
    }
    if !next.is_empty() {
        let look = render::Look {
            glyphs: render::glyphs(),
            ink,
            width: render::stderr_width(),
        };
        eprint!(
            "\n{}",
            render::draw(
                render::doc::Doc::new().with(render::blocks::Next { steps: next }),
                &look
            )
        );
    }
}

/// What decides whether a stream gets color: the command line, and the
/// variables the convention reads, read here once for the whole process.
fn color_policy(when: render::ink::ColorWhen) -> render::ink::ColorPolicy {
    let set = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    render::ink::ColorPolicy {
        when,
        no_color: set("NO_COLOR").is_some(),
        force: set("CLICOLOR_FORCE").is_some_and(|value| value != "0"),
        off: set("CLICOLOR").is_some_and(|value| value == "0"),
        dumb: set("TERM").is_some_and(|value| value == "dumb"),
        links: links(&set),
    }
}

/// Whether a path is a link on a stream with color: what
/// `YUNTA_HYPERLINKS` says outright, or what the terminal announced about
/// itself.
fn links(set: &dyn Fn(&str) -> Option<String>) -> render::ink::Links {
    use render::ink::Links;
    match set("YUNTA_HYPERLINKS").as_deref() {
        Some("1") => return Links::Forced,
        Some("0") => return Links::Refused,
        _ => {}
    }
    let program = set("TERM_PROGRAM");
    let vte = set("VTE_VERSION").and_then(|version| version.parse::<u32>().ok());
    let announced = matches!(program.as_deref(), Some("iTerm.app" | "WezTerm" | "vscode"))
        || vte.is_some_and(|version| version >= 5000)
        || set("KITTY_WINDOW_ID").is_some()
        || set("WT_SESSION").is_some();
    match announced {
        true => Links::Announced,
        false => Links::Unknown,
    }
}

/// The three values the glyph policy reads, as this process was started
/// with them.
fn glyph_env() -> render::glyphs::GlyphEnv {
    render::glyphs::GlyphEnv {
        explicit: std::env::var(render::glyphs::OVERRIDE_VAR).ok(),
        locale: ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty())),
        term: std::env::var("TERM").ok(),
    }
}

/// The width a reader asked every line to be laid out in, when
/// `COLUMNS` names one.
fn columns() -> Option<usize> {
    std::env::var("COLUMNS")
        .ok()
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
fn drive(cli: cli::Cli) -> Result<Outcome, CliError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|source| CliError::io("build the runtime for", "this command", source))?;
    let outcome = runtime.block_on(cli.run());
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
