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

use std::path::Path;
use std::process::ExitCode;

use clap::Parser;

use crate::error::{CliError, Outcome};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = cli::Cli::parse();
    tracing::debug!("yunta starting");

    match cli.run().await {
        Ok(Outcome::Success) => ExitCode::SUCCESS,
        Ok(Outcome::Reported) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
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
