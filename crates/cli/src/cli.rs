//! The command-line surface: the clap command tree and the dispatch that
//! routes each subcommand to its module. `main` only parses argv and maps
//! the result to an exit code; every command lives under `commands/`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use yunta_core::{AdapterId, ModeName, OptionId, PackRef, Responder, RunId};

use crate::commands;
use crate::error::{CliError, Outcome};
use crate::graph;

/// Yunta — a deterministic workflow engine for code agents.
#[derive(Parser)]
#[command(name = "yunta", version, about, arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    /// Runs the parsed subcommand, handing back its verdict or the one
    /// error `main` turns into a line on stderr and a failing exit code.
    pub async fn run(self) -> Result<Outcome, CliError> {
        dispatch(self.command).await
    }
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
        /// own resolution: each role resolves to its candidate on it, and
        /// the log records every candidate passed over. `mock` needs
        /// `--fixture`.
        #[arg(long)]
        adapter: Option<AdapterId>,
        /// With `--adapter mock`: the fixture that scripts every session,
        /// in the same format a `.yunta/tests/` fixture uses.
        #[arg(long, requires = "adapter")]
        fixture: Option<PathBuf>,
        /// Selects a workflow mode. Omitted with `modes:` declared
        /// defaults to the first declared mode; a workflow with no
        /// `modes:` at all ignores this entirely.
        #[arg(long)]
        mode: Option<ModeName>,
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
        /// Prints the run's outcome as one versioned JSON document
        /// instead of the human progress lines — the same DTO the MCP
        /// control plane returns. Suppresses `--follow`'s streaming.
        #[arg(long, conflicts_with = "follow")]
        json: bool,
    },
    /// Shows a run's derived state: nodes, tasks and tokens.
    Status {
        /// The run id (as printed by `yunta run`).
        run_id: RunId,
        /// Prints the derived state as one versioned JSON document
        /// instead of the human view — the same DTO the control plane's
        /// `workflow_status` returns.
        #[arg(long)]
        json: bool,
    },
    /// Resumes a run from its event log, restarting orphaned nodes.
    Resume {
        /// The run id to resume.
        run_id: RunId,
    },
    /// Answers a paused run's gate decision from a separate process — no
    /// live surface attached to the run itself. Records the decision
    /// on the log and hands the run to a detached
    /// resume that applies it: exhausted re-routes (retry/abort/promote)
    /// and unresolved `kind: gate` nodes alike. `yunta status` shows the
    /// pause reason; the option must be on that decision's own menu.
    ResolveGate {
        /// The run id waiting on a decision.
        run_id: RunId,
        /// The chosen option id, as printed by `yunta status`.
        option: OptionId,
        /// Who's answering, for the audit trail
        /// (`gate_resolved.resolved_by`). Omitted, the decision is recorded
        /// as `unverified:$USER` — an ambient identity, not a claimed one.
        #[arg(long)]
        by: Option<Responder>,
        /// Free-form context alongside the choice.
        #[arg(long = "text")]
        free_text: Option<String>,
    },
    /// Sends every running node's session an ordered interrupt,
    /// escalating to `kill` if it doesn't close in time.
    Cancel {
        /// The run id to cancel.
        run_id: RunId,
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
    /// Renders a workflow's DAG as Mermaid or DOT (`--format`) —
    /// optionally annotated with a run's derived state.
    Graph {
        /// Path to the workflow YAML file.
        workflow: PathBuf,
        /// Annotate each node with its derived state from this run.
        #[arg(long)]
        run: Option<RunId>,
        /// Diagram language: `mermaid` (default) or `dot`.
        #[arg(long, default_value = "mermaid")]
        format: graph::GraphFormat,
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
        run_id: RunId,
    },
    /// Generates a Verified Work Receipt for a finished run: markdown
    /// and JSON derived entirely from the event log, written to
    /// the run's own directory and printed to stdout.
    Receipt {
        /// The run id to generate a receipt for.
        run_id: RunId,
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
        run_id: Option<RunId>,
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
    /// Prints the shape of a document Yunta reads and validates, so
    /// nobody has to guess it. With no arguments, lists the kinds.
    Schema {
        /// Which document: `task-ledger`, `findings` or `questions`.
        kind: Option<String>,
        /// Emits the JSON Schema instead of the annotated example — what
        /// an editor's language server validates against.
        #[arg(long)]
        json: bool,
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
        /// Runs the pack's own test cases (`.yunta/tests/`, against the
        /// mock adapter) once it is installed. Without it nothing of the
        /// pack executes during `add`: the audit is read, not run.
        #[arg(long)]
        run_tests: bool,
    },
    /// Re-clones an installed pack at a new ref and re-vendors it.
    Update {
        /// `publisher/name`.
        pack: PackRef,
        r#ref: String,
        /// Confirms updating to a ref that declares executors — same
        /// gate as `add`: a new ref is where new executable code first
        /// appears. `permissions.packs.executors: deny` refuses
        /// regardless; `allow` skips the confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Scaffolds a new pack at `./<name>`: a manifest, one verified
    /// workflow, a self-test config and case, and a README. Checks and
    /// tests it on creation, so the scaffold passes from the first command.
    New {
        /// `publisher/name`.
        pack: PackRef,
    },
    /// Removes a pack's vendored directory and its lock entry.
    Remove {
        /// `publisher/name`.
        pack: PackRef,
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
        pack: PackRef,
    },
}

/// Runs one subcommand, handing back its verdict or the one error the
/// caller turns into a line on stderr and a failing exit code.
async fn dispatch(command: Command) -> Result<Outcome, CliError> {
    match command {
        Command::Check { workflow, config } => commands::check::check(&workflow, config.as_deref()),
        Command::Run {
            workflow,
            input,
            adapter,
            fixture,
            mode,
            follow,
            detach,
            json,
        } => {
            commands::run::run(
                &workflow,
                &input,
                adapter.as_ref(),
                fixture.as_deref(),
                mode.as_ref(),
                follow,
                detach,
                json,
            )
            .await
        }
        Command::Status { run_id, json } => commands::status::status(&run_id, json),
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
                by.as_ref(),
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
        Command::Graph {
            workflow,
            run,
            format,
        } => graph::graph(&workflow, run.as_ref(), format),
        Command::Test { dir } => commands::test::test(dir.as_deref()).await,
        Command::Verify { run_id } => commands::verify::verify(&run_id),
        Command::Receipt { run_id, json } => commands::receipt::receipt(&run_id, json),
        Command::Pack { action } => match action {
            PackAction::Add {
                source,
                yes,
                run_tests,
            } => commands::pack::add(&source, yes, run_tests).await,
            PackAction::Update { pack, r#ref, yes } => {
                commands::pack::update(&pack, &r#ref, yes).await
            }
            PackAction::New { pack } => commands::pack::new_pack(&pack).await,
            PackAction::Remove { pack } => commands::pack::remove(&pack),
            PackAction::List => commands::pack::list(),
            PackAction::Audit { pack } => commands::pack_audit::audit(&pack).await,
        },
        Command::Stats {
            run_id,
            workflow,
            json,
        } => commands::stats::stats(run_id.as_ref(), workflow.as_deref(), json),
        Command::Init { interactive, force } => commands::init::init(interactive, force).await,
        Command::Schema { kind, json } => commands::schema::schema(kind.as_deref(), json),
        Command::New {
            name,
            shape,
            interactive,
            force,
        } => commands::new::new_workflow(&name, shape.as_deref(), interactive, force),
    }
}
