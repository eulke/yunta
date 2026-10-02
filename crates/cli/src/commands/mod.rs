//! One module per subcommand, beside the few things more than one of
//! them shares; `main.rs` only parses and dispatches.

pub(crate) mod advice;
pub mod cancel;
pub mod check;
pub mod close;
pub(crate) mod committer;
pub mod doctor;
pub(crate) mod drive;
pub mod fence;
pub(crate) mod forge;
pub mod gc;
pub mod init;
pub mod list;
pub mod mcp;
pub mod new;
pub mod pack;
pub mod pack_audit;
pub(crate) mod pack_needs;
pub(crate) mod promote;
pub mod receipt;
pub(crate) mod refusals;
pub mod resolve_gate;
pub mod resume;
pub mod run;
pub(crate) mod run_ref;
pub mod schema;
pub mod stats;
pub mod status;
pub mod test;
pub(crate) mod verdict;
pub mod verify;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_adapters::{ClaudeCodeAdapter, CodexAdapter, CLAUDE_CODE_ID, CODEX_ID};
use yunta_core::port::{Adapter, ProbeReport};
use yunta_core::{AdapterId, AdapterSettings, ConfigLayer, Pid, RunId, Workflow};
use yunta_engine::UnknownKindCount;

use crate::error::CliError;

/// A detached `yunta resume` that never started, and the run it was
/// for. Recover by running that command yourself: the run is on disk
/// and unchanged, so nothing is lost by handing it forward by hand.
///
/// Every surface that hands a run off reports the same failure, so the
/// sentence is worded here once and each caller only says what it was
/// doing when it got this back — with the run's whole id, since the
/// control plane is one of those surfaces.
#[derive(Debug, thiserror::Error)]
#[error("cannot spawn a detached `{}`: {source}", advice::resume(.run_id.as_str()))]
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
) -> std::io::Result<Pid> {
    let log_path = run_dir
        .join(yunta_engine::run_dir::SCRATCH_DIR)
        .join("detached.log");
    let log = std::fs::File::create(&log_path)?;
    let log_err = log.try_clone()?;
    let mut child_cmd = tokio::process::Command::new(crate::context::own_binary());
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
        let _ = child.wait().await;
    });
    Ok(pid)
}

/// The adapters a run executes its sessions on, by the name `runners:`
/// reaches each one under. Named beside the function that builds it, so
/// every caller that passes a registry around spells the same type.
pub(crate) type Adapters = HashMap<AdapterId, Arc<dyn Adapter>>;

/// Every adapter this binary builds, with `settings` applied to each —
/// the composition root's one declaration of what a real invocation can
/// run on. Adding an adapter is adding a line here: what `runners:` may
/// name, what `doctor` probes, what `init` offers and what a refusal
/// lists all read from this.
///
/// The mock is not among them. Mock fixtures stay routed through `yunta
/// test`, so a real run never gets a simulated agent.
pub(crate) fn built_adapters(
    settings: impl Fn(&AdapterId) -> AdapterSettings,
) -> Vec<Arc<dyn Adapter>> {
    vec![
        Arc::new(ClaudeCodeAdapter::new(&settings(&CLAUDE_CODE_ID))),
        Arc::new(CodexAdapter::new(&settings(&CODEX_ID))),
    ]
}

/// What every adapter this binary builds declares it can do — what
/// `check` judges a workflow's `permissions:` and `agent:` against.
/// `None` for an adapter this binary does not build: a capability it
/// cannot see is not one it can call absent.
pub(crate) fn declared_capabilities(adapter: &AdapterId) -> Option<yunta_core::Capabilities> {
    built_adapters(|_| AdapterSettings::default())
        .iter()
        .find(|built| built.id() == adapter)
        .map(|built| built.capabilities())
}

/// The adapter this binary built under `id`, constructed without a
/// probe: what the fence hook needs to reach one adapter's codec.
pub(crate) fn built_adapter(id: &AdapterId) -> Option<std::sync::Arc<dyn Adapter>> {
    built_adapters(|_| AdapterSettings::default())
        .into_iter()
        .find(|built| built.id() == id)
}

/// What this binary can run on, as a person reads it: every built
/// adapter's own id, in order, comma-separated — the phrase a refusal
/// ends with, derived rather than written.
pub(crate) fn built_adapter_names() -> String {
    let names: Vec<String> = built_adapters(|_| AdapterSettings::default())
        .iter()
        .map(|adapter| format!("`{}`", adapter.id()))
        .collect();
    names.join(", ")
}

/// The id a surface names when it shows what a `runners:` entry looks
/// like. The first this binary builds, so the example is always an
/// adapter that exists.
pub(crate) fn first_built_adapter() -> AdapterId {
    built_adapters(|_| AdapterSettings::default())
        .first()
        .map(|adapter| adapter.id().clone())
        .unwrap_or_else(|| CLAUDE_CODE_ID.clone())
}

/// The adapters a real invocation offers: those of [`built_adapters`]
/// that `runners:` names as a candidate somewhere in the merged config,
/// each with that adapter's own settings (a `binary` override, if
/// declared).
pub(crate) fn real_adapters(config: &ConfigLayer) -> Adapters {
    let named: Vec<&AdapterId> = config
        .runners
        .iter()
        .flatten()
        .flat_map(|(_, candidates)| candidates.iter())
        .map(|candidate| &candidate.adapter)
        .collect();

    built_adapters(|id| {
        config
            .adapters
            .as_ref()
            .and_then(|adapters| adapters.get(id))
            .cloned()
            .unwrap_or_default()
    })
    .into_iter()
    .filter(|adapter| named.contains(&adapter.id()))
    .map(|adapter| (adapter.id().clone(), adapter))
    .collect()
}

/// Refuses early when `workflow` needs agent sessions no available
/// adapter can provide: an error in check, never emulation at runtime.
pub(crate) fn refuse_unrunnable(workflow: &Workflow, adapters: &Adapters) -> Result<(), CliError> {
    let needs_sessions = workflow.nodes.iter().any(|node| {
        matches!(
            node.kind,
            yunta_core::NodeKind::Prompt { .. } | yunta_core::NodeKind::Loop { .. }
        )
    });
    if needs_sessions && adapters.is_empty() {
        return Err(CliError::msg(format!(
            "this workflow has prompt/loop nodes but `runners:` in the merged config\n\
             names no adapter this binary can run (built: {}). To exercise this\n\
             workflow with the `mock` adapter instead, declare a test case under\n\
             .yunta/tests/ and run `yunta test`.",
            built_adapter_names()
        )));
    }
    Ok(())
}

/// Health-checks every real adapter this run would use via `probe()` —
/// binary present, version compatible, auth valid — and refuses before
/// any worktree or token is spent if one comes back unhealthy. `yunta
/// doctor` calls the same adapters' `probe()` directly instead of
/// through this helper, since it reports every result rather than
/// stopping at the first failure.
pub(crate) async fn probe_or_refuse(adapters: &Adapters) -> Result<(), CliError> {
    let mut unhealthy = Vec::new();
    for (name, adapter) in adapters {
        match adapter.probe().await {
            Ok(ProbeReport::Healthy { .. }) => {}
            // An adapter that reports itself unhealthy without saying
            // why is listed by name alone.
            Ok(ProbeReport::Unhealthy { diagnostic }) => {
                unhealthy.push(yunta_core::text::detailed(name, &diagnostic));
            }
            Err(e) => unhealthy.push(yunta_core::text::detailed(name, &e.to_string())),
        }
    }
    if unhealthy.is_empty() {
        return Ok(());
    }
    Err(CliError::msg(yunta_core::text::problems(
        "adapter health check failed (run `yunta doctor` for detail)",
        &unhealthy,
    )))
}

/// How a partially interpreted run says so: every event kind this
/// binary does not know, with how many events carried it. `None` when
/// the log is interpreted in full.
///
/// `yunta status` folds it into its summary line and `yunta stats`
/// prints it on its own, so the framing is each caller's and the
/// sentence is one — a reader meets the same fact worded the same way
/// on either surface.
pub(crate) fn unknown_kinds_note(
    counts: &[UnknownKindCount],
    glyphs: crate::render::Glyphs,
) -> Option<String> {
    if counts.is_empty() {
        return None;
    }
    let kinds: Vec<String> = counts
        .iter()
        .map(|count| format!("{} {}{}", count.kind, glyphs.times(), count.events))
        .collect();
    Some(format!(
        "{}, interpreted partially: {}",
        yunta_core::text::counted(counts.len(), "unknown event kind"),
        kinds.join(", ")
    ))
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
) -> yunta_engine::ContextFilesCheck {
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
}

/// [`context_files`] for a run started here now, from the commit
/// `ctx.cwd` is on. Every node counts, since nothing has chosen a mode.
/// Nothing to say outside a repository with a commit, where no run starts
/// either.
pub(crate) async fn context_files_at_head(
    ctx: &crate::context::Context,
    workflow: &Workflow,
    isolation: yunta_core::Isolation,
) -> yunta_engine::ContextFilesCheck {
    let Ok(base) = yunta_engine::head_commit(&ctx.cwd, ctx.supervision()).await else {
        return Default::default();
    };
    context_files(ctx, workflow, None, isolation, &base).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::describe;

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

        let refusal = refuse_unrunnable(
            &yunta_core::yaml::parse::<Workflow>(
                "name: w\nnodes:\n  - id: a\n    kind: prompt\n    runner: r\n    prompt: p\n",
            )
            .expect("the workflow parses"),
            &Adapters::new(),
        )
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

    /// The example a surface offers is an adapter that exists, so
    /// copying the line it prints produces a config this binary can run.
    #[test]
    fn the_example_a_surface_offers_is_an_adapter_this_binary_builds() {
        let example = first_built_adapter();
        assert!(
            built_adapters(|_| AdapterSettings::default())
                .iter()
                .any(|adapter| *adapter.id() == example),
            "`{example}` is offered as an example and is not built"
        );
    }
}
