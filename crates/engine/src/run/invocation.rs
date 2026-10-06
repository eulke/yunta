//! What one invocation of a run holds for as long as it lives: the
//! context every step borrows, the variables layered onto each
//! subprocess, and the shared directories those name, made before
//! anything writes there.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use yunta_core::Pid;

use super::{RunCtx, RunEnv, RunError};
use crate::task_cycle::Memo;

/// Builds the run's context and the two invocation-scoped values the
/// scheduler loop needs beside it: the loop's own handle on the root
/// cancellation, which the caller always brings, and the
/// process-registry write error,
/// carried past construction because a failed write has no log to record
/// itself on until the context exists.
pub(super) fn build_ctx(
    env: RunEnv<'_>,
    depth: u32,
) -> (RunCtx<'_>, CancellationToken, Option<std::io::Error>) {
    let RunEnv {
        run_id,
        manifest,
        run_dir,
        worktree,
        adapters,
        storage,
        clock,
        ids,
        human_interaction,
        forge,
        cancel,
        adapter_override,
        ambient,
        secrets,
        observer,
        rail_command,
    } = env;
    // Resolved once, when the run wakes: every append this invocation
    // makes takes the same values back out, and a run that declares no
    // secret builds an empty one and pays nothing.
    let redactor =
        yunta_core::Redactor::of(&manifest.config.secrets, secrets.as_deref().map(|s| s as _));
    let root_cancel = cancel.clone();
    let root_cancel_for_ctx = root_cancel.clone();
    let (registry, registry_error) = match crate::process_registry::ProcessRegistry::create(
        run_dir,
        Pid::current(),
        clock.now(),
    ) {
        Ok(registry) => (Some(std::sync::Arc::new(registry)), None),
        Err(e) => (None, Some(e)),
    };
    // The per-run MCP host outlives every borrow of this invocation, so
    // it owns a clone of the run's clock and of its observer rather than
    // borrowing them — the one injected clock and the one display
    // surface reach the listener's own event appends.
    let clock_for_host = clock.clone();
    let awake = Arc::new(crate::wakefulness::Wakefulness::new(clock.clone()));
    let observer_for_host = observer.clone();
    // One cache of criterion results per invocation, read by every task
    // cycle and by every check a task session asks for through the host.
    let memo = std::sync::Arc::new(Memo::new(manifest.config_hash.clone()));
    let pool = crate::worktree::CheckoutPool::of_run(
        &crate::run_dir::worktrees_root(manifest, run_dir),
        worktree,
        run_dir,
        manifest,
    );
    let registry_for_host = registry.clone();
    let subprocess_vars = super::invocation::subprocess_vars(ambient, &manifest.config);
    let ctx = RunCtx {
        rail_command,
        run_id,
        manifest,
        run_dir,
        worktree,
        adapters,
        storage,
        clock,
        ids,
        memo: memo.clone(),
        pool: pool.clone(),
        human_interaction,
        adapter_override,
        budget_lifted: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        process_registry: registry,
        root_cancel: root_cancel_for_ctx,
        forge,
        depth,
        ambient,
        subprocess_vars: subprocess_vars.clone().into(),
        secrets,
        redactor: redactor.clone(),
        observer,
        unit: None,
        landing: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        awake,
        // One host per execute_run invocation, shared by every session
        // listener; each of them reads and writes through the host's own
        // clone of the log handle.
        run_tools_host: Arc::new(crate::run_tools::RunToolsHost::new(
            &manifest.workflow,
            crate::run_tools::HostOf {
                storage: storage.clone(),
                run_id: run_id.clone(),
                clock: clock_for_host,
                observer: observer_for_host,
                run_dir: run_dir.to_path_buf(),
                max_artifact_bytes: manifest
                    .config
                    .limits
                    .as_ref()
                    .and_then(|limits| limits.max_artifact_bytes),
                redactor,
                memo,
                pool,
                process_registry: registry_for_host,
                subprocess_vars,
                environment: crate::process::execution_environment(ambient),
                worktree: worktree.to_path_buf(),
                suite: crate::run::baseline::suite(manifest).and_then(yunta_core::Suite::of),
            },
        )),
    };
    (ctx, root_cancel, registry_error)
}

/// Every variable layered onto a subprocess of the run: the ambient
/// ones a caller injects, then each shared directory under its variable.
pub(super) fn subprocess_vars(
    ambient: Option<&yunta_core::Env>,
    config: &yunta_core::ConfigLayer,
) -> Vec<(String, String)> {
    let mut vars = ambient
        .map(|ambient| ambient.subprocess_vars.clone())
        .unwrap_or_default();
    vars.extend(
        config
            .shared_dirs()
            .map(|(var, dir)| (var.to_string(), dir.display().to_string())),
    );
    vars
}

/// Makes every shared directory exist before anything is asked to write
/// in it: a session's sandbox keeps a directory writable, it does not
/// create one.
pub(super) async fn share_dirs(config: &yunta_core::ConfigLayer) -> Result<(), RunError> {
    for (_, dir) in config.shared_dirs() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|source| RunError::Io {
                context: format!("create the shared directory `{}`", dir.display()),
                source,
            })?;
    }
    Ok(())
}
