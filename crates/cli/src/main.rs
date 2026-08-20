#![forbid(unsafe_code)]

//! `yunta` binary entrypoint. Stays thin by design (D04): parse args with
//! clap, delegate everything else to the library crates and the command
//! modules.

mod commands;
mod graph;
mod human_interaction;
mod project;

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
    /// Creates a run from a workflow and executes it.
    Run {
        /// Path to the workflow YAML file.
        workflow: PathBuf,
        /// Sets a declared input: `--input name=value`, repeatable.
        #[arg(long = "input", value_name = "name=value")]
        input: Vec<String>,
        /// Runs every session with this adapter instead of `runners:`'s
        /// own resolution. `mock` is refused here — see `yunta test`.
        #[arg(long)]
        adapter: Option<String>,
        /// Selects a workflow mode (§10.1, D44). Omitted with `modes:`
        /// declared defaults to the first declared mode; a workflow
        /// with no `modes:` at all ignores this entirely.
        #[arg(long)]
        mode: Option<String>,
        /// Prints progress (§8.5) as the run advances, polling the event
        /// log every 500ms instead of only at the end.
        #[arg(long)]
        follow: bool,
        /// Creates the run, then hands it off to a detached `yunta
        /// resume` child and returns immediately with the run id — the
        /// workflow keeps running independent of this invocation (M8,
        /// D101: what `run_workflow` triggers internally so the MCP
        /// control plane never blocks for a run's duration). Mutually
        /// exclusive with `--follow` (there is nothing left in this
        /// process to follow).
        #[arg(long, conflicts_with = "follow")]
        detach: bool,
    },
    /// Shows a run's derived state: nodes, tasks and tokens.
    Status {
        /// The run id (as printed by `yunta run`).
        run_id: String,
    },
    /// Resumes a run from its event log, restarting orphaned nodes.
    Resume {
        /// The run id to resume.
        run_id: String,
    },
    /// Sends every running node's session an ordered interrupt,
    /// escalating to `kill` if it doesn't close in time (Spec Adapter
    /// §2, A4).
    Cancel {
        /// The run id to cancel.
        run_id: String,
    },
    /// Lists workflows under `.yunta/workflows/`, or local runs with
    /// `--runs`.
    List {
        /// Lists local runs and their derived state instead of
        /// workflows.
        #[arg(long)]
        runs: bool,
    },
    /// Health-checks every adapter this project's `runners:` names —
    /// binary present, version compatible, auth valid (Spec Adapter §2).
    Doctor,
    /// Removes orphaned run and worktree directories, respecting
    /// `storage.retention_days`.
    Gc {
        /// Reports what would be removed without removing it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Renders a workflow's DAG as Mermaid — optionally annotated with a
    /// run's derived state.
    Graph {
        /// Path to the workflow YAML file.
        workflow: PathBuf,
        /// Annotate each node with its derived state from this run.
        #[arg(long)]
        run: Option<String>,
    },
    /// Runs the workflow test cases under .yunta/tests/ with the mock
    /// adapter.
    Test,
    /// Verifies a run's event hash chain (I26): integrity and order,
    /// recomputed from the log as persisted.
    Verify {
        /// The run id to verify.
        run_id: String,
    },
    /// Shows verification cost stats: one run (`run_id`) or a workflow's
    /// own history (`--workflow`), never both.
    Stats {
        /// The run id to inspect.
        run_id: Option<String>,
        /// Aggregates every past run of this workflow instead of one run.
        #[arg(long, conflicts_with = "run_id")]
        workflow: Option<String>,
        /// Prints machine-readable JSON instead of the terminal view.
        #[arg(long)]
        json: bool,
    },
    /// Prepares this repo for Yunta once: detects ecosystem, base branch
    /// and available adapters, writes `.yunta/config.yaml` and the
    /// mechanism skill.
    Init {
        /// Prompts to confirm/override detected values (degrades to
        /// non-interactive without a TTY).
        #[arg(short, long)]
        interactive: bool,
        /// Overwrites an existing `.yunta/config.yaml`.
        #[arg(long)]
        force: bool,
    },
    /// Writes `.yunta/workflows/<name>.yaml` from a commented schema
    /// skeleton and runs `check` on it.
    New {
        /// The workflow's name — becomes `.yunta/workflows/<name>.yaml`.
        name: String,
        /// Which skeleton to start from: one-node, lint-fix or ledger.
        #[arg(long)]
        shape: Option<String>,
        /// Prompts to choose a shape when `--shape` is omitted (degrades
        /// to `one-node` without a TTY).
        #[arg(short, long)]
        interactive: bool,
        /// Overwrites an existing workflow file of the same name.
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
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
        Some(Command::Run {
            workflow,
            input,
            adapter,
            mode,
            follow,
            detach,
        }) => {
            commands::run::run(
                &workflow,
                &input,
                adapter.as_deref(),
                mode.as_deref(),
                follow,
                detach,
            )
            .await
        }
        Some(Command::Status { run_id }) => commands::status::status(&run_id),
        Some(Command::Resume { run_id }) => commands::resume::resume(&run_id).await,
        Some(Command::Cancel { run_id }) => commands::cancel::cancel(&run_id).await,
        Some(Command::List { runs }) => {
            if runs {
                commands::list::list_runs()
            } else {
                commands::list::list_workflows()
            }
        }
        Some(Command::Doctor) => commands::doctor::doctor().await,
        Some(Command::Gc { dry_run }) => commands::gc::gc(dry_run),
        Some(Command::Graph { workflow, run }) => graph::graph(&workflow, run.as_deref()),
        Some(Command::Test) => commands::test::test().await,
        Some(Command::Verify { run_id }) => commands::verify::verify(&run_id),
        Some(Command::Stats {
            run_id,
            workflow,
            json,
        }) => commands::stats::stats(run_id.as_deref(), workflow.as_deref(), json),
        Some(Command::Init { interactive, force }) => {
            commands::init::init(interactive, force).await
        }
        Some(Command::New {
            name,
            shape,
            interactive,
            force,
        }) => commands::new::new_workflow(&name, shape.as_deref(), interactive, force),
    }
}

pub(crate) fn load_yaml<T: serde::de::DeserializeOwned>(
    path: &Path,
    what: &str,
) -> Result<T, ExitCode> {
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

    // Without `--config`, check sees the project's real layers (§2.2) —
    // the same ones a run would — including the `permissions` layer
    // conflict check (§6.1: a lower layer re-permitting what a higher one
    // denied is refused here, citing both layers). An explicit `--config`
    // is a single already-merged file: nothing layered to conflict.
    let config: ConfigLayer = match config_path {
        Some(path) => match load_yaml(path, "config") {
            Ok(c) => c,
            Err(code) => return code,
        },
        None => {
            let cwd = match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(e) => {
                    eprintln!("error: cannot determine the current directory: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let layers = match project::load_named_layers(&cwd) {
                Ok(layers) => layers,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let named: Vec<(&str, &ConfigLayer)> =
                layers.iter().map(|(name, layer)| (*name, layer)).collect();
            let conflicts = yunta_core::permission_layer_conflicts(&named);
            if !conflicts.is_empty() {
                eprintln!("{}: {} error(s)", workflow_path.display(), conflicts.len());
                for conflict in &conflicts {
                    eprintln!("  {conflict}");
                }
                return ExitCode::FAILURE;
            }
            ConfigLayer::merge_layers(layers.into_iter().map(|(_, layer)| layer))
        }
    };

    let mut errors = yunta_engine::check(&workflow, &config);
    // T9.3: composition references resolve against the repo catalog
    // under the current directory (`.yunta/workflows/`).
    if let Ok(cwd) = std::env::current_dir() {
        errors.extend(yunta_engine::check_workflow_refs(&workflow, &config, &cwd));
    }
    let warnings = yunta_engine::check_warnings(&workflow, &config);
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }

    // §8.7/T7.10: "en el momento en que alguien ya está tocando ese
    // workflow" — surfaced here too, not just `stats --workflow`. Best
    // effort: a project with no state root yet (nothing ever ran) or an
    // unnamed workflow simply shows nothing, same as `list_workflows`'s
    // own stance on missing history.
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(project) = project::resolve(&cwd) {
            if let Ok(storage) = yunta_storage::Storage::open(&project.storage_path) {
                let (history, _) =
                    commands::stats::collect_raw_history(&project, &storage, &workflow.name);
                let findings =
                    yunta_engine::analyze_verification_effectiveness(&workflow, &history);
                let text = commands::stats::render_verification_findings(&findings);
                if !text.is_empty() {
                    eprintln!("\n{text}");
                }
            }
        }
    }

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
