//! One module per subcommand, beside the few things more than one of
//! them shares; `main.rs` only parses and dispatches.

pub mod cancel;
pub mod check;
pub mod close;
pub(crate) mod committer;
pub mod doctor;
pub(crate) mod drive;
pub(crate) mod forge;
pub mod gc;
pub mod init;
pub mod list;
pub mod mcp;
pub mod new;
pub mod open;
pub mod pack;
pub(crate) mod promote;
pub mod rail;
pub mod receipt;
pub(crate) mod refusals;
pub(crate) mod registry;
pub mod resolve_gate;
pub mod resume;
pub mod review;
pub mod run;
pub(crate) mod run_ref;
pub mod schema;
pub mod stats;
pub mod status;
pub mod test;
pub(crate) mod verdict;
pub mod verify;

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use yunta_core::events::PauseReason;
use yunta_core::port::ProbeReport;
use yunta_core::step::NextStep;
use yunta_core::{Pid, RunId, Workflow};
use yunta_render::command;
use yunta_replay::RunState;

use crate::error::CliError;
use crate::output::Mode;
use surco::look::Look;

/// A detached `yunta resume` that never started, and the run it was
/// for. Recover by running that command yourself: the run is on disk
/// and unchanged, so nothing is lost by handing it forward by hand.
///
/// Every surface that hands a run off reports the same failure, so the
/// sentence is worded here once and each caller only says what it was
/// doing when it got this back — with the run's whole id, since the
/// control plane is one of those surfaces.
#[derive(Debug, thiserror::Error)]
#[error("cannot spawn a detached `{}`: {source}", yunta_render::command::line_for(&yunta_core::step::NextStep::Resume { run: RunId::clone(.run_id) }, yunta_render::command::Reader::Agent))]
pub(crate) struct DetachedResumeError {
    run_id: RunId,
    #[source]
    source: std::io::Error,
}

impl DetachedResumeError {
    /// The failure of a hand-off for `run_id`, keeping what the OS said
    /// about it.
    pub(crate) fn new(run_id: &RunId, source: std::io::Error) -> Self {
        Self {
            run_id: run_id.clone(),
            source,
        }
    }
}

/// Hands a run off to a fully independent `yunta resume` and returns
/// without waiting on it — what `run --detach`, `resolve-gate` and the
/// MCP `run_workflow`/`resume_run` tools all need (a control-plane
/// operation that must never block for the run's own duration). No new
/// execution path: the detached child is an ordinary resume, exactly what
/// a human would run by hand. Its own log goes to
/// `run.dir/scratch/detached.log` (never silently discarded); its process
/// group is its own, so a signal to *this* invocation's group (a shell's
/// Ctrl-C) can never reach it.
///
/// The launcher still owns reaping: a background task holds the child
/// handle and awaits its exit, so a finished detached run never lingers
/// as a zombie. `run --detach` exits right after and the child reparents
/// to init; the long-lived MCP server would otherwise accumulate the
/// defunct children of every run it started, so this owner task is what
/// collects them.
pub(crate) async fn spawn_detached_resume(
    run_dir: &Path,
    run_id: &str,
    cwd: &Path,
    binary: &Path,
) -> std::io::Result<Pid> {
    let log_path = run_dir
        .join(yunta_engine::run_dir::SCRATCH_DIR)
        .join("detached.log");
    let log = detached_log(&log_path).await?;
    let log_err = log.try_clone()?;
    let mut child_cmd = tokio::process::Command::new(binary);
    child_cmd
        .arg("resume")
        .arg(run_id)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(log_err);
    #[cfg(unix)]
    child_cmd.process_group(0);
    let mut child = child_cmd.spawn()?;
    // Who the run was handed to. A caller holding a claim on something
    // the child is about to work in — the checkout's own lock, under
    // `isolation: none` — has to move it, and cannot without a name.
    let pid = child
        .id()
        .and_then(|id| Pid::try_from(id).ok())
        .ok_or_else(|| std::io::Error::other("the detached child reported no usable process id"))?;
    tokio::spawn(async move {
        // best-effort: the reaper only collects the child; its own log says how the run went.
        let _ = child.wait().await;
    });
    Ok(pid)
}

/// The detached log at `path`, opened to add to it: a run that pauses on a
/// reset again hands itself to a later link of the same chain, and each
/// link's log follows the one before it instead of erasing it.
async fn detached_log(path: &Path) -> std::io::Result<std::fs::File> {
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    Ok(file.into_std().await)
}

/// The instant a run paused on a limit's reset resumes at, or `None` for
/// any other run. A resume of such a run waits for that instant before it
/// takes anything.
pub(crate) fn reset_at(state: &RunState) -> Option<DateTime<Utc>> {
    match state.run.paused() {
        Some((PauseReason::AwaitingReset { at, .. }, _)) => Some(*at),
        _ => None,
    }
}

/// Refuses early when one of `nodes`, the nodes a run reaches, needs an
/// agent session and no available adapter can provide one: an error in
/// check, never emulation at runtime.
pub(crate) fn refuse_unrunnable(
    nodes: &[&yunta_core::Node],
    adapters: &yunta_api::machine::Adapters,
) -> Result<(), CliError> {
    let needs_sessions = nodes.iter().any(|node| node.opens_session());
    if needs_sessions && adapters.is_empty() {
        return Err(CliError::msg(format!(
            "this workflow has prompt/loop nodes but `runners:` in the merged config\n\
             names no adapter this binary can run (built: {built}). To exercise this\n\
             workflow with the `mock` adapter instead, declare a test case under\n\
             .yunta/tests/ and run `{test}`.",
            built = yunta_api::machine::built_adapter_names(),
            test = command::line(&NextStep::Test { dir: None })
        )));
    }
    Ok(())
}

/// Health-checks every real adapter this run would use via `probe()` —
/// the binary there and answering `--version`, every setting read — and
/// refuses before any worktree or token is spent if one comes back
/// unhealthy. `yunta doctor` calls the same adapters' `probe()` directly
/// instead of through this helper, since it reports every result as a
/// check of its own rather than refusing.
pub(crate) async fn probe_or_refuse(
    adapters: &yunta_api::machine::Adapters,
) -> Result<(), CliError> {
    let mut unhealthy = Vec::new();
    for (name, adapter) in adapters {
        match adapter.probe().await {
            Ok(ProbeReport::Healthy { .. }) => {}
            // An adapter that reports itself unhealthy without saying
            // why is listed by name alone.
            Ok(ProbeReport::Unhealthy { diagnostic }) => {
                unhealthy.push(surco::text::detailed(name, &diagnostic));
            }
            Err(e) => unhealthy.push(surco::text::detailed(name, &e.to_string())),
        }
    }
    if unhealthy.is_empty() {
        return Ok(());
    }
    Err(CliError::msg(surco::text::problems(
        format!(
            "adapter health check failed (run `{}` for detail)",
            command::line(&NextStep::Doctor {
                sessions: false,
                full: false
            })
        ),
        &unhealthy,
    )))
}

/// Resolves a workflow reference to a file, the one rule `check`, `run`
/// and `graph` share: a bare catalog name (no extension) resolves through
/// the repo catalog under `cwd`, then a publisher's vendored packs
/// (`acme/review`); anything carrying an extension is taken as a literal
/// path. `cwd` is injected rather than read here so each command resolves
/// against the directory it already established.
pub(crate) fn resolve_workflow_ref(cwd: &Path, reference: &Path) -> Result<PathBuf, CliError> {
    if reference.extension().is_none() {
        Ok(yunta_engine::resolve_workflow(cwd, &reference.to_string_lossy())?.path)
    } else {
        Ok(reference.to_path_buf())
    }
}

/// The literal `files:` paths the nodes of `workflow` that `mode_nodes`
/// includes (every node when `None`) read and a run started at
/// `ctx.cwd` from `base` would not find: refused when nothing that runs
/// before the reader can write them, warned about when something might.
pub(crate) async fn context_files(
    ctx: &crate::context::Context,
    workflow: &Workflow,
    mode_nodes: Option<&std::collections::HashSet<yunta_core::NodeId>>,
    isolation: yunta_core::Isolation,
    base: &yunta_core::CommitSha,
) -> Result<yunta_engine::ContextFilesCheck, CliError> {
    yunta_engine::check_context_files(
        workflow,
        mode_nodes,
        yunta_engine::RunTreeOrigin {
            checkout: &ctx.cwd,
            isolation,
            base,
        },
        ctx.supervision(),
    )
    .await
    .map_err(CliError::from)
}

/// [`context_files`] for a run started here now, from the commit
/// `ctx.cwd` is on. Every node counts, since nothing has chosen a mode.
/// Nothing to say outside a repository with a commit, where no run starts
/// either; a stop while asking for that commit is the command's stop.
pub(crate) async fn context_files_at_head(
    ctx: &crate::context::Context,
    workflow: &Workflow,
    isolation: yunta_core::Isolation,
) -> Result<yunta_engine::ContextFilesCheck, CliError> {
    let base = match yunta_engine::head_commit(&ctx.cwd, ctx.supervision()).await {
        Ok(base) => base,
        Err(stopped) if stopped.cancelled() => {
            return Err(yunta_engine::RunError::from(stopped).into())
        }
        Err(_) => return Ok(Default::default()),
    };
    context_files(ctx, workflow, None, isolation, &base).await
}

/// The line a run starts with: its whole id — the record a script and a
/// log keep — what is happening to it, and the command that follows it
/// by its handle. Under `--quiet`, the id and nothing else; under `--json`,
/// nothing, because the document is the whole answer.
pub(crate) fn started(run_id: &yunta_core::RunId, doing: &str, output: Mode, look: &Look) {
    match output {
        Mode::Human => crate::output::say(
            look,
            format!(
                "run {run_id}: {doing} (`{}` follows it)",
                command::line(&NextStep::Follow {
                    run: run_id.clone()
                })
            ),
        ),
        Mode::Quiet => crate::output::say(look, run_id.to_string()),
        Mode::Json => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_api::machine::{built_adapters, Adapters};
    use yunta_core::{describe, AdapterSettings};

    #[tokio::test]
    async fn a_later_link_of_a_detached_chain_adds_to_the_log() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("detached.log");
        for line in ["first link\n", "second link\n"] {
            detached_log(&path)
                .await
                .unwrap()
                .write_all(line.as_bytes())
                .unwrap();
        }
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, "first link\nsecond link\n");
    }

    /// Every sentence that tells a person which adapters exist reads
    /// from the one place they are declared. Before this, three of them
    /// spelled the pair out, so a third adapter would have landed with
    /// the refusal, the probe listing and the `init` template all still
    /// naming two.
    #[test]
    fn what_a_refusal_names_is_what_this_binary_builds() {
        let built = built_adapters(|_| AdapterSettings::default());
        assert!(
            !built.is_empty(),
            "a binary with no adapter can run no session"
        );

        let workflow = yunta_core::yaml::parse::<Workflow>(
            "name: w\nnodes:\n  - id: a\n    kind: prompt\n    runner: r\n    prompt: p\n",
        )
        .expect("the workflow parses");
        let refusal =
            refuse_unrunnable(&workflow.iter_nodes().collect::<Vec<_>>(), &Adapters::new())
                .expect_err("a prompt node with no adapter is unrunnable");

        let text = describe(&refusal);
        for adapter in &built {
            assert!(
                text.contains(adapter.id().as_str()),
                "the refusal names `{}`, which this binary builds: {text}",
                adapter.id()
            );
        }
    }
}
