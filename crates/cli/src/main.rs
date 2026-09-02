//! `yunta` binary entrypoint. Stays thin by design: parse args with
//! clap, delegate everything else to the library crates and the command
//! modules.

mod commands;
mod graph;
mod human_interaction;
mod pack;
mod project;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use yunta_core::{ConfigLayer, Workflow};

/// Yunta — a deterministic workflow engine for code agents.
#[derive(Parser)]
#[command(name = "yunta", version, about, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
        /// Selects a workflow mode. Omitted with `modes:` declared
        /// defaults to the first declared mode; a workflow with no
        /// `modes:` at all ignores this entirely.
        #[arg(long)]
        mode: Option<String>,
        /// Prints progress as the run advances, polling the event log
        /// every 500ms instead of only at the end.
        #[arg(long)]
        follow: bool,
        /// Creates the run, then hands it off to a detached `yunta
        /// resume` child and returns immediately with the run id — the
        /// workflow keeps running independent of this invocation (the
        /// same thing `run_workflow` triggers internally so the MCP
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
    /// Answers a paused run's gate decision from a separate process — no
    /// live surface attached to the run itself. Records the decision
    /// on the log and hands the run to a detached
    /// resume that applies it: exhausted re-routes (retry/abort/promote)
    /// and unresolved `kind: gate` nodes alike. `yunta status` shows the
    /// pause reason; the option must be on that decision's own menu.
    ResolveGate {
        /// The run id waiting on a decision.
        run_id: String,
        /// The chosen option id, as printed by `yunta status`.
        option: String,
        /// Who's answering, for the audit trail (`gate_resolved.resolved_by`).
        #[arg(long)]
        by: Option<String>,
        /// Free-form context alongside the choice.
        #[arg(long = "text")]
        free_text: Option<String>,
    },
    /// Sends every running node's session an ordered interrupt,
    /// escalating to `kill` if it doesn't close in time.
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
    /// binary present, version compatible, auth valid.
    Doctor,
    /// Runs the control-plane MCP server over stdio: `list_workflows`,
    /// `run_workflow`, `workflow_status`, `resume_run`, `resolve_gate`
    /// — none of which ever blocks for a run's own duration. Not a
    /// daemon: exits when the client closes stdin, and no run's own
    /// life depends on this process staying up.
    Mcp,
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
    Test {
        /// Project root whose `.yunta/` holds the cases, workflows and
        /// config; defaults to the current directory.
        #[arg(long, value_name = "path")]
        dir: Option<PathBuf>,
    },
    /// Verifies a run's event hash chain: integrity and order,
    /// recomputed from the log as persisted.
    Verify {
        /// The run id to verify.
        run_id: String,
    },
    /// Generates a Verified Work Receipt for a finished run: markdown
    /// and JSON derived entirely from the event log, written to
    /// the run's own directory and printed to stdout.
    Receipt {
        /// The run id to generate a receipt for.
        run_id: String,
        /// Prints the JSON receipt instead of the markdown one.
        #[arg(long)]
        json: bool,
    },
    /// Installs, updates, removes and lists third-party packs.
    Pack {
        #[command(subcommand)]
        action: PackAction,
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

#[derive(Subcommand)]
enum PackAction {
    /// Clones, vendors to `.yunta/packs/<publisher>/<name>/`, and locks a
    /// pack. `<source>` is a git URL or `host/publisher/name` shorthand,
    /// optionally suffixed `@<ref>` (tag, branch or commit-ish).
    Add {
        source: String,
        /// Confirms installing a pack that ships executors (executable
        /// code, not just declarative YAML) — required whenever the
        /// pack's `declares.executors` is non-empty; review the audit
        /// this command prints first.
        #[arg(long)]
        yes: bool,
    },
    /// Re-clones an installed pack at a new ref and re-vendors it.
    Update {
        /// `publisher/name`.
        publisher_name: String,
        r#ref: String,
        /// Confirms updating to a ref that declares executors — same
        /// gate as `add`: a new ref is where new executable code first
        /// appears. `permissions.packs.executors: deny` refuses
        /// regardless; `allow` skips the confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Removes a pack's vendored directory and its lock entry.
    Remove {
        /// `publisher/name`.
        publisher_name: String,
    },
    /// Lists every locked pack, verifying its vendored content against
    /// the lock.
    List,
    /// Full static inventory of an installed pack: every
    /// command, context source, per-node permission, required agent,
    /// mcp server, executor, and each workflow's full prompt text —
    /// plus whether the pack ships tests and whether they pass.
    /// `add` runs this automatically before vendoring; this is the
    /// on-demand form.
    Audit {
        /// `publisher/name`.
        publisher_name: String,
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
        Command::Check { workflow, config } => run_check(&workflow, config.as_deref()),
        Command::Run {
            workflow,
            input,
            adapter,
            mode,
            follow,
            detach,
        } => {
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
        Command::Status { run_id } => commands::status::status(&run_id),
        Command::Resume { run_id } => commands::resume::resume(&run_id).await,
        Command::ResolveGate {
            run_id,
            option,
            by,
            free_text,
        } => {
            commands::resolve_gate::resolve_gate(
                &run_id,
                &option,
                by.as_deref(),
                free_text.as_deref(),
            )
            .await
        }
        Command::Cancel { run_id } => commands::cancel::cancel(&run_id).await,
        Command::List { runs } => {
            if runs {
                commands::list::list_runs()
            } else {
                commands::list::list_workflows()
            }
        }
        Command::Doctor => commands::doctor::doctor().await,
        Command::Mcp => commands::mcp::mcp().await,
        Command::Gc { dry_run } => commands::gc::gc(dry_run),
        Command::Graph { workflow, run } => graph::graph(&workflow, run.as_deref()),
        Command::Test { dir } => commands::test::test(dir.as_deref()).await,
        Command::Verify { run_id } => commands::verify::verify(&run_id),
        Command::Receipt { run_id, json } => commands::receipt::receipt(&run_id, json),
        Command::Pack { action } => match action {
            PackAction::Add { source, yes } => commands::pack::add(&source, yes).await,
            PackAction::Update {
                publisher_name,
                r#ref,
                yes,
            } => commands::pack::update(&publisher_name, &r#ref, yes).await,
            PackAction::Remove { publisher_name } => commands::pack::remove(&publisher_name),
            PackAction::List => commands::pack::list(),
            PackAction::Audit { publisher_name } => {
                commands::pack_audit::audit(&publisher_name).await
            }
        },
        Command::Stats {
            run_id,
            workflow,
            json,
        } => commands::stats::stats(run_id.as_deref(), workflow.as_deref(), json),
        Command::Init { interactive, force } => commands::init::init(interactive, force).await,
        Command::New {
            name,
            shape,
            interactive,
            force,
        } => commands::new::new_workflow(&name, shape.as_deref(), interactive, force),
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
    // A bare catalog name (no `.yaml`/`.yml` extension) resolves through
    // the repo catalog, then a publisher's vendored packs — same rule
    // `yunta run` follows; anything with an extension stays a literal
    // path.
    let resolved_path: std::path::PathBuf;
    let workflow_path: &Path = if workflow_path.extension().is_none() {
        let cwd = match std::env::current_dir() {
            Ok(cwd) => cwd,
            Err(e) => {
                eprintln!("error: cannot determine the current directory: {e}");
                return ExitCode::FAILURE;
            }
        };
        match yunta_engine::resolve_workflow(&cwd, &workflow_path.to_string_lossy()) {
            Ok(resolved) => {
                resolved_path = resolved.path;
                &resolved_path
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        workflow_path
    };

    let workflow: Workflow = match load_yaml(workflow_path, "workflow") {
        Ok(w) => w,
        Err(code) => return code,
    };

    // Without `--config`, check sees the project's real layers — the
    // same ones a run would — including the `permissions` layer
    // conflict check (a lower layer re-permitting what a higher one
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
    // Composition references resolve against the repo catalog under
    // the current directory (`.yunta/workflows/`), then packs.
    if let Ok(cwd) = std::env::current_dir() {
        let origin = yunta_engine::origin_of(&cwd, workflow_path);
        errors.extend(yunta_engine::check_workflow_refs(
            &workflow, &config, &cwd, &origin,
        ));
    }
    let warnings = yunta_engine::check_warnings(&workflow, &config);
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }

    // Verification-effectiveness findings, surfaced here too — right
    // when someone is already looking at this workflow — not just via
    // `stats --workflow`. Best effort: a project with no state root
    // yet (nothing ever ran) or an unnamed workflow simply shows
    // nothing, same as `list_workflows`'s own stance on missing
    // history.
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
