//! Routes each parsed subcommand to the module under `commands/` that
//! runs it.

use std::path::PathBuf;
use std::time::Instant;

use yunta_core::{OptionId, Responder, WorkflowName};

use super::{Command, PackAction};
use crate::commands;
use crate::commands::run_ref::{named, RunArg, RunRef};
use crate::error::{CliError, Outcome};
use crate::graph;
use crate::output::{self, Mode};
use crate::render::Look;

/// Runs one subcommand, handing back its verdict or the one error the
/// caller turns into a line on stderr and a failing exit code. `started`
/// is when this process began, which a hook's window counts from. `output`
/// is the mode the subcommand's own flags chose, and `look` the stdout look
/// `main` built: every command writes with those and nothing else.
pub(super) async fn dispatch(
    command: Command,
    started: Instant,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    match command {
        Command::Completions { shell } => completions_command(shell),
        Command::Check { workflow, config } => {
            commands::check::check(&workflow, config.as_deref(), look).await
        }
        Command::Run {
            workflow,
            input,
            adapter,
            fixture,
            mode,
            detach,
            ..
        } => {
            commands::run::run(
                &workflow,
                &input,
                adapter.as_ref(),
                fixture.as_deref(),
                mode.as_ref(),
                output,
                detach,
                look,
            )
            .await
        }
        Command::Status { run, node, .. } => {
            commands::status::status(&run.named().await?, output, node.as_ref(), look).await
        }
        Command::Resume { run, .. } => {
            commands::resume::resume(&run.named().await?, output, look).await
        }
        Command::ResolveGate {
            run,
            option,
            by,
            free_text,
            ..
        } => resolve_gate_command(run, option, by, free_text, output, look).await,
        Command::Cancel { run, .. } => {
            commands::cancel::cancel(&run.named().await?, output, look).await
        }
        Command::Close { run, by, .. } => {
            commands::close::close(&run.named().await?, by.as_ref(), output, look).await
        }
        Command::List { runs, all, .. } => list_command(runs, all, output, look).await,
        Command::Doctor { session, .. } => commands::doctor::doctor(session, output, look).await,
        Command::Mcp => commands::mcp::mcp().await,
        Command::Gc { dry_run } => commands::gc::gc(dry_run, look).await,
        Command::Graph {
            workflow,
            run,
            format,
        } => graph_command(workflow, run, format).await,
        Command::Test { dir } => commands::test::test(dir.as_deref(), look).await,
        Command::Verify { run, .. } => {
            commands::verify::verify(&run.named().await?, output, look).await
        }
        Command::Receipt { run, .. } => {
            commands::receipt::receipt(&run.named().await?, output, look).await
        }
        Command::Pack { action } => pack_command(action, look).await,
        Command::Stats { run, workflow, .. } => stats_command(run, workflow, output, look).await,
        Command::Init { interactive, force } => {
            commands::init::init(interactive, force, look).await
        }
        Command::Rail { adapter, event } => rail_hook(&adapter, event, started),
        Command::Schema { kind, .. } => commands::schema::schema(kind.as_deref(), output, look),
        Command::New {
            name,
            shape,
            interactive,
            force,
        } => commands::new::new_workflow(&name, shape.as_deref(), interactive, force, look).await,
    }
}

/// `yunta completions`: the completion script for `shell`, on stdout.
fn completions_command(shell: clap_complete::Shell) -> Result<Outcome, CliError> {
    use clap::CommandFactory;
    output::completions(shell, &mut super::Cli::command());
    Ok(Outcome::Success)
}

/// `yunta resolve-gate`: answers the gate of the run it names.
async fn resolve_gate_command(
    run: RunArg,
    option: Option<OptionId>,
    by: Option<Responder>,
    free_text: Option<String>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    commands::resolve_gate::resolve_gate(
        &run.named().await?,
        option.as_ref(),
        by.as_ref(),
        free_text.as_deref(),
        output,
        look,
    )
    .await
}

/// `yunta list`: the runs when `runs` is set, the workflows otherwise.
async fn list_command(
    runs: bool,
    all: bool,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    if runs {
        commands::list::list_runs(all, output, look).await
    } else {
        commands::list::list_workflows(look).await
    }
}

/// `yunta graph`, for the workflow or the run it names.
async fn graph_command(
    workflow: Option<PathBuf>,
    run: Option<RunRef>,
    format: graph::GraphFormat,
) -> Result<Outcome, CliError> {
    graph::graph(
        workflow.as_deref(),
        named(run.as_ref()).await?.as_ref(),
        format,
    )
    .await
}

/// `yunta stats`, for the run or the workflow it names.
async fn stats_command(
    run: Option<RunRef>,
    workflow: Option<WorkflowName>,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    commands::stats::stats(
        named(run.as_ref()).await?.as_ref(),
        workflow.as_ref(),
        output,
        look,
    )
    .await
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
        PackAction::Audit { pack } => commands::pack_audit::audit(&pack, look).await,
    }
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
