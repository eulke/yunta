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
mod error;
mod graph;
mod human_interaction;
mod identity;
mod json;
mod pack;
mod project;
mod render;
mod surface;

use std::path::Path;
use std::process::ExitCode;

use clap::Parser;

use crate::error::{CliError, Outcome};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = cli::Cli::parse();
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
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
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

/// Reads and parses a YAML file into `T`, naming what it was reading and
/// where when it can't — the one loader every command reaches for.
pub(crate) fn load_yaml<T: serde::de::DeserializeOwned>(
    path: &Path,
    what: &str,
) -> Result<T, CliError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|source| CliError::io(&format!("read {what} at"), path.display(), source))?;
    yunta_core::yaml::parse(&contents)
        .map_err(|e| CliError::msg(format!("failed to parse {what} at {}: {e}", path.display())))
}
