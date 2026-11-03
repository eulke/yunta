//! The command-line surface: the clap command tree and the dispatch that
//! routes each subcommand to its module. `main` only parses argv and maps
//! the result to an exit code; every command lives under `commands/`.

use std::path::PathBuf;
use std::time::Instant;

use clap::{Args, Parser, Subcommand};
use surco::look::Look;
use yunta_core::{AdapterId, ModeName, OptionId, Responder};

use crate::commands::run_ref::{RunArg, RunRef};
use crate::error::{CliError, Outcome};
use crate::graph;
use crate::output::Mode;

/// Yunta: a deterministic workflow engine for code agents.
#[derive(Parser)]
#[command(name = "yunta", version, about)]
pub struct Cli {
    /// The subcommand; without one, `yunta` shows the runs of the
    /// repository it is run in.
    #[command(subcommand)]
    command: Option<Command>,
    /// When to color what is printed: on a terminal that draws it
    /// (`auto`, honoring `YUNTA_LOOK`, `NO_COLOR`, `CLICOLOR` and
    /// `CLICOLOR_FORCE`), `always`, or `never`.
    ///
    /// The colors are 24-bit where `YUNTA_LOOK=24` asks for them or
    /// `COLORTERM` says the terminal draws them, tuned to the ground
    /// `COLORFGBG` names; otherwise they are the sixteen of the terminal's
    /// theme.
    #[arg(long, global = true, value_enum, default_value_t)]
    pub(crate) color: surco::paint::ColorWhen,
}

mod depth;
mod dispatch;
mod pack;

use depth::DepthArg;
use dispatch::dispatch;
use pack::PackAction;

impl Cli {
    /// The output mode of the parsed subcommand, read from its own flags.
    pub(crate) fn mode(&self) -> Mode {
        match &self.command {
            Some(command) => Mode::of(command.json(), command.quiet()),
            None => Mode::Human,
        }
    }

    /// Runs the parsed subcommand, handing back its verdict or the one
    /// error `main` turns into a line on stderr and a failing exit code.
    /// `started` is when this process began, which a hook's window counts
    /// from; `output` and `look` are how this process writes, as `main`
    /// decided them.
    pub async fn run(
        self,
        started: Instant,
        output: Mode,
        look: &Look,
    ) -> Result<Outcome, CliError> {
        dispatch(self.command, started, output, look).await
    }
}

#[derive(Subcommand)]
enum Command {
    /// Check a workflow without running it.
    Check {
        /// Path to the workflow YAML file.
        workflow: PathBuf,
        /// Optional config YAML file (runners, adapters, storage, paths).
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Create a run from a workflow and execute it.
    Run(RunArgs),
    /// Show where a run stands: nodes, tasks, tokens.
    Status {
        #[command(flatten)]
        run: RunArg,
        /// Prints the derived state as one versioned JSON document
        /// instead of the human view: the same DTO the control plane's
        /// `workflow_status` returns.
        #[arg(long, conflicts_with_all = ["full", "dig"])]
        json: bool,
        /// Shows one node whole: its whole failure or what its agent said,
        /// the end of what it printed and where all of it is, and what it
        /// produced.
        #[arg(long, value_name = "id", conflicts_with_all = ["json", "full", "dig"])]
        node: Option<yunta_core::NodeId>,
        #[command(flatten)]
        depth: DepthArg,
    },
    /// Resume a run from its event log.
    ///
    /// Resumes a run from its event log, restarting orphaned nodes. A run
    /// paused on a limit's reset waits for the reset first.
    Resume {
        #[command(flatten)]
        run: RunArg,
        /// Prints the run id and nothing else, the same way `run` does.
        #[arg(long, conflicts_with = "json")]
        quiet: bool,
        /// Prints the run's outcome as one versioned JSON document:
        /// the same document `run --json` prints, because the two
        /// commands execute the same thing.
        #[arg(long)]
        json: bool,
    },
    /// Answer the decision a parked run waits on.
    ///
    /// Answers a paused run's gate decision from a separate process, with
    /// no live surface attached to the run itself. Records the decision
    /// on the log and hands the run to a detached
    /// resume that applies it: exhausted re-routes (retry/abort/promote)
    /// and unresolved `kind: gate` nodes alike. `yunta status` shows the
    /// pause reason; the option must be on that decision's own menu.
    ResolveGate(ResolveGateArgs),
    /// Stop a running run and its sessions.
    ///
    /// Sends every running node's session an ordered interrupt,
    /// escalating to `kill` if it doesn't close in time.
    Cancel {
        #[command(flatten)]
        run: RunArg,
        /// Prints the answer as one versioned JSON document instead of the
        /// terminal view.
        #[arg(long)]
        json: bool,
    },
    /// Close a stopped run nobody will continue.
    ///
    /// Closes a stopped run nobody is going to continue, as cancelled,
    /// so it stops waiting on a person. A run whose engine died is
    /// settled first, as `cancel` settles one.
    Close {
        #[command(flatten)]
        run: RunArg,
        /// Who is closing it, for the audit trail (`run_finished.closed_by`).
        /// Omitted, the close is recorded as `unverified:$USER`.
        #[arg(long)]
        by: Option<Responder>,
        /// Prints the answer as one versioned JSON document instead of the
        /// terminal view.
        #[arg(long)]
        json: bool,
    },
    /// List workflows, or runs with `--runs`.
    ///
    /// Lists workflows under `.yunta/workflows/`, or local runs with
    /// `--runs`.
    List(ListArgs),
    /// Check adapters, git, the forge and packs.
    ///
    /// Health-checks every adapter this project's `runners:` names:
    /// binary present, version compatible, auth valid.
    Doctor {
        /// Also opens one real session per binding any runner names (the
        /// smallest run there is, through the same machinery a workflow
        /// uses, run tools mounted) and reports how each one ended. Spends one prompt per binding.
        #[arg(long)]
        session: bool,
        /// Lists every check on its own row: a glance folds the ones that
        /// hold into one.
        #[arg(long)]
        full: bool,
        /// Prints the report as one versioned JSON document instead of the
        /// terminal view.
        #[arg(long)]
        json: bool,
    },
    /// Serve the control plane over MCP on stdio.
    ///
    /// Runs the control-plane MCP server over stdio: `document_shape`,
    /// `list_workflows`, `run_workflow`, `workflow_status`,
    /// `resume_run`, `resolve_gate`, `answer_questions`, none of which
    /// ever blocks for a run's own duration. Not a daemon: exits when
    /// the client closes stdin, and no run's own life depends on this
    /// process staying up.
    Mcp,
    /// Remove orphaned run and worktree directories.
    ///
    /// Removes orphaned run and worktree directories, respecting
    /// `storage.retention_days`.
    Gc {
        /// Reports what would be removed without removing it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Draw a workflow's graph as Mermaid or DOT.
    ///
    /// Renders a workflow's DAG as Mermaid or DOT (`--format`),
    /// optionally annotated with a run's derived state.
    Graph {
        /// Path to the workflow YAML file, or a catalog name. Omitted
        /// with `--run`, which draws the workflow that run froze.
        workflow: Option<PathBuf>,
        /// Draw the workflow this run froze, each node annotated with
        /// the state its own log derives, named as any command names a
        /// run.
        #[arg(long)]
        run: Option<RunRef>,
        /// Diagram language: `mermaid` (default) or `dot`.
        #[arg(long, default_value = "mermaid")]
        format: graph::GraphFormat,
    },
    /// Run the workflow test cases in .yunta/tests.
    ///
    /// Runs the workflow test cases under .yunta/tests/ with the mock
    /// adapter.
    Test {
        /// Project root whose `.yunta/` holds the cases, workflows and
        /// config; defaults to the current directory.
        #[arg(long, value_name = "path")]
        dir: Option<PathBuf>,
    },
    /// Verify a run's event chain and artifacts.
    ///
    /// Verifies a run's evidence: the event hash chain, recomputed from
    /// the log as persisted, and the bytes of every artifact that log
    /// accepted, read back and hashed against its own name.
    Verify {
        #[command(flatten)]
        run: RunArg,
        /// Prints the verification as one versioned JSON document instead
        /// of the terminal view.
        #[arg(long)]
        json: bool,
    },
    /// Write a finished run's Verified Work Receipt.
    ///
    /// Generates a Verified Work Receipt for a finished run: markdown
    /// and JSON derived entirely from the event log, written to
    /// the run's own directory and printed to stdout.
    Receipt {
        #[command(flatten)]
        run: RunArg,
        /// Prints the JSON receipt instead of the markdown one.
        #[arg(long)]
        json: bool,
    },
    /// Open a run's page in a browser.
    ///
    /// Writes the run's page into its folder, with each document a decision
    /// it waits on is about, whole, and opens it. A page runs nothing: its
    /// buttons copy commands. Run from a script, it says where the page is.
    Open {
        #[command(flatten)]
        run: RunArg,
        /// Prints where the page is as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Install, update, remove and list packs.
    Pack {
        #[command(subcommand)]
        action: PackAction,
    },
    /// Show what a run or a workflow cost.
    ///
    /// Shows verification cost stats: one run (`run_id`) or a workflow's
    /// own history (`--workflow`), never both.
    Stats {
        /// The run: its handle, its id or any part that starts or ends it,
        /// `last` (this repository's newest) or `needs` (the one waiting on
        /// you).
        run: Option<RunRef>,
        /// Aggregates every past run of this workflow instead of one run.
        #[arg(long, conflicts_with = "run")]
        workflow: Option<yunta_core::WorkflowName>,
        /// Prints machine-readable JSON instead of the terminal view.
        #[arg(long)]
        json: bool,
        /// How much of the run's stats to tell: `--full` adds its rails and
        /// each node's blocked share and models.
        #[command(flatten)]
        depth: DepthArg,
    },
    /// Prepare this repository for Yunta.
    ///
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
    /// Answer an agent CLI's hook.
    ///
    /// What an agent's CLI runs while it works (before it writes, among
    /// other moments): reads the call on stdin, asks the engine of the
    /// session it belongs to, and answers in that CLI's own shape.
    /// Hidden because no person invokes it: a session's own CLI does,
    /// and the adapter that opened the session is what put the command
    /// there.
    #[command(name = yunta_core::rail::SUBCOMMAND, hide = true)]
    Rail {
        /// The adapter whose codec reads this call.
        adapter: yunta_core::AdapterId,
        /// When in the session the CLI asks.
        event: yunta_core::rail::RailEvent,
    },
    /// Show the shape of a document Yunta reads.
    ///
    /// Prints the shape of a document Yunta reads and validates, so
    /// nobody has to guess it. With no arguments, lists the documents.
    Schema {
        /// Which document; with none, every one is listed.
        document: Option<String>,
        /// Emits the JSON Schema instead of the page a person reads: what
        /// an editor's language server validates against.
        #[arg(long)]
        json: bool,
    },
    /// Print a shell's completion script.
    ///
    /// Prints the script that completes `yunta`'s commands and flags in
    /// `shell`: source it from the shell's startup file, or write it
    /// where the shell loads completions from.
    Completions {
        /// The shell to complete in.
        shell: clap_complete::Shell,
    },
    /// Write a new workflow from a skeleton.
    ///
    /// Writes `.yunta/workflows/<name>.yaml` from a commented schema
    /// skeleton and runs `check` on it.
    New(NewArgs),
}

/// What `yunta run` takes: the workflow, the inputs it declares, and
/// how it runs.
#[derive(Args)]
struct RunArgs {
    /// Path to the workflow YAML file.
    workflow: PathBuf,
    /// Sets a declared input: `--input name=value`, repeatable.
    #[arg(long = "input", value_name = "name=value")]
    input: Vec<String>,
    /// Runs every session with this adapter instead of `runners:`'s
    /// own resolution: each role resolves to its candidate on it, and
    /// the log records every candidate passed over. The run keeps it
    /// through every resume. `mock` needs `--fixture` and runs
    /// attached, never with `--detach`.
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
    /// Prints the run id and nothing else: no progress, and no
    /// closing block. The verdict travels in the exit code. A
    /// diagnostic and the budget warning are printed either way:
    /// quiet is about progress, not about problems.
    #[arg(long, conflicts_with = "json")]
    quiet: bool,
    /// Creates the run, then hands it off to a detached `yunta
    /// resume` child and returns immediately with the run id; the
    /// workflow keeps running independent of this invocation (the
    /// same thing `run_workflow` triggers internally so the MCP
    /// control plane never blocks for a run's duration).
    #[arg(long)]
    detach: bool,
    /// Prints the run's outcome as one versioned JSON document
    /// instead of the human progress and closing block: the same
    /// DTO the MCP control plane returns.
    #[arg(long)]
    json: bool,
}

/// What `yunta resolve-gate` takes: the run, and how its gate is
/// answered.
#[derive(Args)]
struct ResolveGateArgs {
    #[command(flatten)]
    run: RunArg,
    /// The chosen option id, as printed by `yunta status`. Without
    /// one, the run's own menu is put to this terminal.
    option: Option<OptionId>,
    /// Who's answering, for the audit trail
    /// (`gate_resolved.resolved_by`). Omitted, the decision is recorded
    /// as `unverified:$USER`, an ambient identity, not a claimed one.
    #[arg(long)]
    by: Option<Responder>,
    /// Free-form context alongside the choice.
    #[arg(long = "text")]
    free_text: Option<String>,
    /// Prints the answer as one versioned JSON document instead of the
    /// terminal view. The option is then required: the run's own menu
    /// is put to a terminal.
    #[arg(long)]
    json: bool,
}

/// What `yunta list` takes: which listing to show, and how much of it.
#[derive(Args)]
struct ListArgs {
    /// Lists local runs and their derived state instead of
    /// workflows: the runs of the repository this is run in.
    #[arg(long)]
    runs: bool,
    /// With `--runs`: every run on this machine, whichever repository
    /// it was created in.
    #[arg(long, requires = "runs")]
    all: bool,
    /// With `--runs`: the runs as one versioned JSON document instead
    /// of the terminal view.
    #[arg(long, requires = "runs", conflicts_with_all = ["full", "dig"])]
    json: bool,
    /// With `--runs`: lists the runs that closed and the ones that do
    /// not read back, which a glance only counts.
    #[arg(long, requires = "runs")]
    full: bool,
    /// With `--runs`: what `--full` lists, each run named by its whole
    /// id.
    #[arg(long, requires = "runs")]
    dig: bool,
}

/// What `yunta new` takes: the workflow's name, the skeleton it starts
/// from, and whether an existing file is overwritten.
#[derive(Args)]
struct NewArgs {
    /// The workflow's name; it becomes `.yunta/workflows/<name>.yaml`.
    name: String,
    /// Which skeleton to start from: one-node, lint-fix or tasks.
    #[arg(long)]
    shape: Option<String>,
    /// Prompts to choose a shape when `--shape` is omitted (degrades
    /// to `one-node` without a TTY).
    #[arg(short, long)]
    interactive: bool,
    /// Overwrites an existing workflow file of the same name.
    #[arg(long)]
    force: bool,
}

impl Command {
    /// Whether this subcommand answers with one JSON document.
    fn json(&self) -> bool {
        match self {
            Command::Run(args) => args.json,
            Command::ResolveGate(args) => args.json,
            Command::List(args) => args.json,
            Command::Status { json, .. }
            | Command::Resume { json, .. }
            | Command::Cancel { json, .. }
            | Command::Close { json, .. }
            | Command::Doctor { json, .. }
            | Command::Verify { json, .. }
            | Command::Receipt { json, .. }
            | Command::Open { json, .. }
            | Command::Stats { json, .. }
            | Command::Schema { json, .. } => *json,
            _ => false,
        }
    }

    /// Whether this subcommand prints the run id and nothing else.
    fn quiet(&self) -> bool {
        match self {
            Command::Run(args) => args.quiet,
            Command::Resume { quiet, .. } => *quiet,
            _ => false,
        }
    }
}
