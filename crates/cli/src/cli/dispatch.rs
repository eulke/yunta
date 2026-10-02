//! Routes each parsed subcommand to the module under `commands/` that
//! runs it.

use super::{Command, PackAction};
use crate::commands;
use crate::commands::run_ref::named;
use crate::error::{CliError, Outcome};
use crate::graph;

/// Runs one subcommand, handing back its verdict or the one error the
/// caller turns into a line on stderr and a failing exit code.
pub(super) async fn dispatch(command: Command) -> Result<Outcome, CliError> {
    match command {
        Command::Check { workflow, config } => {
            commands::check::check(&workflow, config.as_deref()).await
        }
        Command::Run {
            workflow,
            input,
            adapter,
            fixture,
            mode,
            quiet,
            detach,
            json,
        } => {
            commands::run::run(
                &workflow,
                &input,
                adapter.as_ref(),
                fixture.as_deref(),
                mode.as_ref(),
                quiet,
                detach,
                json,
            )
            .await
        }
        Command::Status { run, json, node } => {
            commands::status::status(&run.named().await?, json, node.as_ref()).await
        }
        Command::Resume { run, quiet, json } => {
            commands::resume::resume(&run.named().await?, quiet, json).await
        }
        Command::ResolveGate {
            run,
            option,
            by,
            free_text,
        } => {
            commands::resolve_gate::resolve_gate(
                &run.named().await?,
                &option,
                by.as_ref(),
                free_text.as_deref(),
            )
            .await
        }
        Command::Cancel { run } => commands::cancel::cancel(&run.named().await?).await,
        Command::Close { run, by } => {
            commands::close::close(&run.named().await?, by.as_ref()).await
        }
        Command::List { runs, all } => {
            if runs {
                commands::list::list_runs(all).await
            } else {
                commands::list::list_workflows().await
            }
        }
        Command::Doctor { session } => commands::doctor::doctor(session).await,
        Command::Mcp => commands::mcp::mcp().await,
        Command::Gc { dry_run } => commands::gc::gc(dry_run),
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
        Command::Test { dir } => commands::test::test(dir.as_deref()).await,
        Command::Verify { run } => commands::verify::verify(&run.named().await?).await,
        Command::Receipt { run, json } => {
            commands::receipt::receipt(&run.named().await?, json).await
        }
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
            run,
            workflow,
            json,
        } => {
            commands::stats::stats(named(run.as_ref()).await?.as_ref(), workflow.as_ref(), json)
                .await
        }
        Command::Init { interactive, force } => commands::init::init(interactive, force).await,
        Command::Fence { adapter } => fence_hook(&adapter),
        Command::Schema { kind, json } => commands::schema::schema(kind.as_deref(), json),
        Command::New {
            name,
            shape,
            interactive,
            force,
        } => commands::new::new_workflow(&name, shape.as_deref(), interactive, force).await,
    }
}

/// Runs the hook and leaves the CLI that called it exactly what its own
/// protocol expects: the streams, and the exit code as the outcome.
///
/// Reading stdin happens here and the environment at the shell's one
/// boundary; the judgement itself is a pure function below both.
fn fence_hook(adapter: &yunta_core::AdapterId) -> Result<Outcome, CliError> {
    use std::io::{Read, Write};

    let mut stdin = Vec::new();
    std::io::stdin().read_to_end(&mut stdin).map_err(|source| {
        CliError::msg(format!("the fence hook cannot read its call: {source}"))
    })?;
    let built = commands::built_adapter(adapter);
    let env = crate::project::process_env();
    let reply = commands::fence::run(
        built.as_ref().and_then(|built| built.fence_codec()),
        env.fence_var.as_deref(),
        &stdin,
    );
    // A hook that cannot deliver its answer has not answered, and the
    // exit code alone is what the calling CLI then reads: a failure
    // here leaves the refusal, which is the safe side of it.
    std::io::stdout()
        .write_all(&reply.stdout)
        .and_then(|()| std::io::stderr().write_all(&reply.stderr))
        .map_err(|source| {
            CliError::msg(format!("the fence hook cannot answer its call: {source}"))
        })?;
    Ok(Outcome::Code(u8::try_from(reply.exit).unwrap_or(1)))
}
