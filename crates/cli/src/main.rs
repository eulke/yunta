#![forbid(unsafe_code)]

//! `yunta` binary entrypoint. Stays thin by design (D04): parse args with
//! clap, delegate everything else to the library crates. Subcommands
//! (`run`, `check`, `status`, `resume`, ...) land in M-0/M7 tasks; for now
//! this only proves the `cli → engine` edge of the workspace graph (T0.1).

use clap::Parser;

/// Yunta — a deterministic workflow engine for code agents.
#[derive(Parser)]
#[command(name = "yunta", version, about)]
struct Cli;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let _cli = Cli::parse();
    tracing::debug!("yunta starting");
    println!("{}", yunta_engine::version_string());
}
