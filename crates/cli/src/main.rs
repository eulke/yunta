#![forbid(unsafe_code)]

//! `yunta` binary entrypoint. Stays thin by design (D04): parse args with
//! clap, delegate everything else to the library crates.
//!
//! Only `check` is wired so far. `run`/`status`/`resume` need pieces
//! `yunta check` doesn't (a manifest/`run_created`, T1.4; and for `run`,
//! how a `loop` node finds its ledger and how a real invocation would
//! route adapter fixtures — neither is settled yet) — see
//! `docs/m0-status.md` for the open questions before wiring them.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use yunta_core::{ConfigLayer, Workflow};

/// Yunta — a deterministic workflow engine for code agents.
#[derive(Parser)]
#[command(name = "yunta", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Validates a workflow statically, without running it.
    Check {
        /// Path to the workflow YAML file.
        workflow: PathBuf,
        /// Optional config YAML file (runners, adapters, storage, paths).
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    tracing::debug!("yunta starting");

    match cli.command {
        None => {
            println!("{}", yunta_engine::version_string());
            ExitCode::SUCCESS
        }
        Some(Command::Check { workflow, config }) => run_check(&workflow, config.as_deref()),
    }
}

fn load_yaml<T: serde::de::DeserializeOwned>(path: &Path, what: &str) -> Result<T, ExitCode> {
    let contents = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: failed to read {what} at {}: {e}", path.display());
        ExitCode::FAILURE
    })?;
    serde_yaml::from_str(&contents).map_err(|e| {
        eprintln!("error: failed to parse {what} at {}: {e}", path.display());
        ExitCode::FAILURE
    })
}

fn run_check(workflow_path: &Path, config_path: Option<&Path>) -> ExitCode {
    let workflow: Workflow = match load_yaml(workflow_path, "workflow") {
        Ok(w) => w,
        Err(code) => return code,
    };

    let config: ConfigLayer = match config_path {
        Some(path) => match load_yaml(path, "config") {
            Ok(c) => c,
            Err(code) => return code,
        },
        None => ConfigLayer::default(),
    };

    let errors = yunta_engine::check(&workflow, &config);
    if errors.is_empty() {
        println!("{}: OK", workflow_path.display());
        ExitCode::SUCCESS
    } else {
        eprintln!("{}: {} error(s)", workflow_path.display(), errors.len());
        for error in &errors {
            eprintln!("  {error}");
        }
        ExitCode::FAILURE
    }
}
