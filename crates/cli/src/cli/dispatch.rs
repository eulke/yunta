//! Routes each parsed subcommand to the module under `commands/` that
//! runs it.

use std::path::PathBuf;
use std::time::Instant;

use surco::doc::Depth;
use surco::look::Look;
use yunta_core::{Responder, WorkflowName};

use super::{Command, ListArgs, NewArgs, PackAction, ResolveGateArgs, RunArgs};
use crate::commands::run_ref::{named, RunArg, RunRef};
use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{self, Mode};
use crate::{commands, graph};

/// Runs one subcommand, handing back its verdict or the one error the
/// caller turns into a line on stderr and a failing exit code. `started`
/// is when this process began, which a hook's window counts from. `output`
/// is the mode the subcommand's own flags chose, and `look` the stdout look
/// `main` built: every command writes with those and nothing else.
pub(super) async fn dispatch(
    command: Option<Command>,
    started: Instant,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let Some(command) = command else {
        return commands::list::field(look).await;
    };
    match command {
        Command::Completions { shell } => completions_command(shell),
        Command::Check { workflow, config } => check_command(workflow, config, look).await,
        Command::Run(args) => run_command(args, output, look).await,
        Command::Status { run, follow, .. } if follow => {
            commands::status::follow::follow(&run.named().await?, look).await
        }
        Command::Status {
            run, node, depth, ..
        } => status_command(run, node, depth.depth(), output, look).await,
        Command::Resume { run, .. } => resume_command(run, output, look).await,
        Command::ResolveGate(args) => resolve_gate_command(args, output, look).await,
        Command::Review { run } => review_command(run, output, look).await,
        Command::Cancel { run, .. } => cancel_command(run, output, look).await,
        Command::Close { run, by, .. } => close_command(run, by, output, look).await,
        Command::List(args) => list_command(args, output, look).await,
        Command::Doctor { session, full, .. } => doctor_command(session, full, output, look).await,
        Command::Mcp => commands::mcp::mcp().await,
        Command::Gc { dry_run } => commands::gc::gc(dry_run, look).await,
        Command::Graph {
            workflow,
            run,
            format,
        } => graph_command(workflow, run, format, look).await,
        Command::Test { dir } => commands::test::test(dir.as_deref(), look).await,
        Command::Verify { run, .. } => verify_command(run, output, look).await,
        Command::Receipt { run, .. } => receipt_command(run, output, look).await,
        Command::Open { run, .. } => open_command(run, output, look).await,
        Command::Pack { action } => pack_command(action, look).await,
        Command::Stats {
            run,
            workflow,
            depth,
            ..
        } => stats_command(run, workflow, depth.depth(), output, look).await,
        Command::Init { interactive, force } => init_command(interactive, force, look).await,
        Command::Rail { adapter, event } => rail_hook(&adapter, event, started),
        Command::Schema { document, .. } => schema_command(document, output, look),
        Command::New(args) => new_command(args, look).await,
    }
}

/// `yunta completions`: the completion script for `shell`, on stdout.
fn completions_command(shell: clap_complete::Shell) -> Result<Outcome, CliError> {
    use clap::CommandFactory;
    output::completions(shell, &mut super::Cli::command());
    Ok(Outcome::Success)
}

/// `yunta check`: checks the workflow it names without running it.
async fn check_command(
    workflow: PathBuf,
    config: Option<PathBuf>,
    look: &Look,
) -> Result<Outcome, CliError> {
    commands::check::check(&workflow, config.as_deref(), look).await
}

/// `yunta run`: creates the run its workflow names and drives it here, or
/// hands it to a detached child that outlives this process.
async fn run_command(args: RunArgs, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    commands::run::run(
        &args.workflow,
        &args.input,
        args.adapter.as_ref(),
        args.fixture.as_deref(),
        args.mode.as_ref(),
        output,
        args.detach,
        look,
    )
    .await
}

/// `yunta status`: where the run stands, or, with `--node`, one node whole.
async fn status_command(
    run: RunArg,
    node: Option<yunta_core::NodeId>,
    depth: Depth,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let page = (node.as_ref(), depth);
    commands::status::status(&run.named().await?, output, page, look).await
}

/// `yunta resume`: resumes the run it names from its event log.
async fn resume_command(run: RunArg, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    commands::resume::resume(&run.named().await?, output, look).await
}

/// `yunta resolve-gate`: answers the gate of the run it names.
async fn resolve_gate_command(
    args: ResolveGateArgs,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    commands::resolve_gate::resolve_gate(
        &args.run.named().await?,
        args.option.as_ref(),
        args.by.as_ref(),
        args.free_text.as_deref(),
        output,
        look,
    )
    .await
}

/// `yunta review`: reads the plan the run it names waits on, and answers it.
async fn review_command(run: RunArg, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    commands::review::review(&run.named().await?, output, look).await
}

/// `yunta cancel`: stops the run it names and its sessions.
async fn cancel_command(run: RunArg, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    commands::cancel::cancel(&run.named().await?, output, look).await
}

/// `yunta close`: closes a stopped run nobody will continue, as cancelled.
async fn close_command(
    run: RunArg,
    by: Option<Responder>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    commands::close::close(&run.named().await?, by.as_ref(), output, look).await
}

/// `yunta list`: the runs when `runs` is set, the workflows otherwise.
async fn list_command(args: ListArgs, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    if args.runs {
        commands::list::list_runs(args.all, Depth::of(args.full, args.dig), output, look).await
    } else {
        commands::list::list_workflows(look).await
    }
}

/// `yunta doctor`: health-checks the adapters, git, the forge and the packs.
async fn doctor_command(
    session: bool,
    full: bool,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    commands::doctor::doctor(session, full, output, look).await
}

/// `yunta graph`, for the workflow or the run it names.
async fn graph_command(
    workflow: Option<PathBuf>,
    run: Option<RunRef>,
    format: graph::GraphFormat,
    look: &Look,
) -> Result<Outcome, CliError> {
    graph::graph(
        workflow.as_deref(),
        named(run.as_ref()).await?.as_ref(),
        format,
        look,
    )
    .await
}

/// `yunta verify`: checks the run's event chain and its artifacts.
async fn verify_command(run: RunArg, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    commands::verify::verify(&run.named().await?, output, look).await
}

/// `yunta open`: writes the run's page and opens it.
async fn open_command(run: RunArg, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    commands::open::open(&run.named().await?, output, look).await
}

/// `yunta receipt`: writes the Verified Work Receipt of the finished run.
async fn receipt_command(run: RunArg, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    commands::receipt::receipt(&run.named().await?, output, look).await
}

/// `yunta stats`, for the run or the workflow it names: clap requires a run
/// unless `--workflow` is named, and refuses the two together.
async fn stats_command(
    run: Option<RunRef>,
    workflow: Option<WorkflowName>,
    depth: Depth,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    match (run, workflow) {
        (Some(run), None) => {
            let run_id = run.resolve(&crate::context::Context::load()?).await?;
            commands::stats::of_run(&run_id, depth, output, look).await
        }
        (None, Some(workflow)) => commands::stats::of_workflow(&workflow, output, look).await,
        // Clap refuses both and neither before this runs; a parser that let one
        // through is refused the same way, not trusted.
        (Some(_), Some(_)) | (None, None) => Err(CliError::msg(
            "stats reads one run or one workflow's history: name exactly one",
        )),
    }
}

/// `yunta pack`: each pack subcommand, run by the module that owns it.
async fn pack_command(action: PackAction, look: &Look) -> Result<Outcome, CliError> {
    match action {
        PackAction::Add {
            source,
            yes,
            run_tests,
        } => commands::pack::add(&source, yes, run_tests, look).await,
        PackAction::Update { pack, r#ref, yes } => {
            commands::pack::update(&pack, &r#ref, yes, look).await
        }
        PackAction::New { pack } => commands::pack::new_pack(&pack, look).await,
        PackAction::Remove { pack } => commands::pack::remove(&pack, look),
        PackAction::List => commands::pack::list(look),
        PackAction::Audit { pack } => commands::pack::audit(&pack, look).await,
    }
}

/// `yunta init`: prepares this repository for Yunta, once.
async fn init_command(interactive: bool, force: bool, look: &Look) -> Result<Outcome, CliError> {
    commands::init::init(interactive, force, look).await
}

/// `yunta schema`: prints the shape of the document it names, or lists them.
fn schema_command(
    document: Option<String>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    commands::schema::schema(document.as_deref(), output, look)
}

/// `yunta new`: writes a new workflow from a skeleton.
async fn new_command(args: NewArgs, look: &Look) -> Result<Outcome, CliError> {
    commands::new::new_workflow(
        &args.name,
        args.shape.as_deref(),
        args.interactive,
        args.force,
        look,
    )
    .await
}

/// Runs the hook and leaves the CLI that called it exactly what its own
/// protocol expects: the streams, and the exit code as the outcome.
///
/// Reading stdin happens here and the environment at the shell's one
/// boundary; what the hook answers comes from the engine it asks.
fn rail_hook(
    adapter: &yunta_core::AdapterId,
    event: yunta_core::rail::RailEvent,
    started: Instant,
) -> Result<Outcome, CliError> {
    use std::io::Read;

    let mut stdin = Vec::new();
    std::io::stdin()
        .read_to_end(&mut stdin)
        .map_err(|source| CliError::msg(format!("the hook cannot read its call: {source}")))?;
    let built = commands::built_adapter(adapter);
    let env = crate::project::process_env();
    let reply = commands::rail::run(
        built.as_ref().and_then(|built| built.rail_codec()),
        event,
        env.rail.as_ref(),
        &stdin,
        started,
    );
    // A hook that cannot deliver its answer has not answered, and the
    // exit code alone is what the calling CLI then reads: a failure
    // here leaves the refusal, which is the safe side of it.
    output::hook_reply(&reply.stdout, &reply.stderr)
        .map_err(|source| CliError::msg(format!("the hook cannot answer its call: {source}")))?;
    Ok(Outcome::Code(u8::try_from(reply.exit).unwrap_or(1)))
}
