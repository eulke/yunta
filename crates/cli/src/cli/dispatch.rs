//! Routes each parsed subcommand to the module under `commands/` that
//! runs it.

use std::time::Instant;

use super::{Command, PackAction};
use crate::commands;
use crate::commands::run_ref::named;
use crate::error::{CliError, Outcome};
use crate::graph;
use crate::output::Mode;
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
        Command::Completions { shell } => {
            use clap::CommandFactory;
            clap_complete::generate(
                shell,
                &mut super::Cli::command(),
                "yunta",
                &mut std::io::stdout(),
            );
            Ok(Outcome::Success)
        }
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
        } => {
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
        Command::Cancel { run, .. } => {
            commands::cancel::cancel(&run.named().await?, output, look).await
        }
        Command::Close { run, by, .. } => {
            commands::close::close(&run.named().await?, by.as_ref(), output, look).await
        }
        Command::List { runs, all, .. } => {
            if runs {
                commands::list::list_runs(all, output, look).await
            } else {
                commands::list::list_workflows(look).await
            }
        }
        Command::Doctor { session, .. } => commands::doctor::doctor(session, output, look).await,
        Command::Mcp => commands::mcp::mcp().await,
        Command::Gc { dry_run } => commands::gc::gc(dry_run, look).await,
        Command::Graph {
            workflow,
            run,
            format,
        } => {
            graph::graph(
                workflow.as_deref(),
                named(run.as_ref()).await?.as_ref(),
                format,
            )
            .await
        }
        Command::Test { dir } => commands::test::test(dir.as_deref(), look).await,
        Command::Verify { run, .. } => {
            commands::verify::verify(&run.named().await?, output, look).await
        }
        Command::Receipt { run, .. } => {
            commands::receipt::receipt(&run.named().await?, output, look).await
        }
        Command::Pack { action } => match action {
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
        },
        Command::Stats { run, workflow, .. } => {
            commands::stats::stats(
                named(run.as_ref()).await?.as_ref(),
                workflow.as_ref(),
                output,
                look,
            )
            .await
        }
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
    use std::io::{Read, Write};

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
    std::io::stdout()
        .write_all(&reply.stdout)
        .and_then(|()| std::io::stderr().write_all(&reply.stderr))
        .map_err(|source| CliError::msg(format!("the hook cannot answer its call: {source}")))?;
    Ok(Outcome::Code(u8::try_from(reply.exit).unwrap_or(1)))
}
