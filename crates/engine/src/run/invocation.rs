//! What one invocation of a run holds for as long as it lives: the
//! context every step borrows, the variables layered onto each
//! subprocess, and the shared directories those name, made before
//! anything writes there.

use std::path::Path;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use yunta_core::protected::Protected;
use yunta_core::Pid;

use super::{RunCtx, RunEnv, RunError};
use crate::task_cycle::Memo;

/// Builds the run's context and the two invocation-scoped values the
/// scheduler loop needs beside it: the loop's own handle on the root
/// cancellation, which the caller always brings, and the
/// process-registry write error,
/// carried past construction because a failed write has no log to record
/// itself on until the context exists. A run whose declared secrets refuse
/// it is refused here, before the registry or the pool writes anything.
pub(super) fn build_ctx(
    env: RunEnv<'_>,
    depth: u32,
) -> Result<(RunCtx<'_>, CancellationToken, Option<std::io::Error>), RunError> {
    // Resolved once, when the run wakes and before anything is written:
    // every append this invocation makes takes the same values back out,
    // and a run that declares no secret builds an empty one and pays nothing.
    let redactor = yunta_core::Redactor::of(&env.manifest.config.secret_names(), &*env.secrets)?;
    let (registry, registry_error) = open_registry(env.run_dir, &*env.clock);
    // One cache of criterion results per invocation, read by every task
    // cycle and by every check a task session asks for through the host.
    let memo = Arc::new(Memo::new(env.manifest.config_hash.clone()));
    let pool = crate::worktree::CheckoutPool::of_run(
        &crate::run_dir::worktrees_root(env.manifest, env.run_dir),
        env.worktree,
        env.run_dir,
        env.manifest,
        env.clock.clone(),
    );
    let subprocess_vars = super::invocation::subprocess_vars(env.ambient, &env.manifest.config);
    let run_tools_host = mcp_host(&env, &redactor, &memo, &pool, &registry, &subprocess_vars);
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
        ambient,
        secrets,
        observer,
        rail_command,
    } = env;
    let awake = Arc::new(crate::wakefulness::Wakefulness::new(clock.clone()));
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
        memo,
        pool,
        human_interaction,
        budget_lifted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        process_registry: registry,
        root_cancel: cancel.clone(),
        forge,
        depth,
        ambient,
        subprocess_vars: subprocess_vars.into(),
        secrets,
        redactor,
        observer,
        unit: None,
        landing: Arc::new(tokio::sync::Mutex::new(())),
        awake,
        protected: Arc::new(std::sync::RwLock::new(Arc::new(Protected::default()))),
        run_tools_host,
    };
    Ok((ctx, cancel.clone(), registry_error))
}

/// Opens the run's process registry. A failed write is returned beside the
/// context rather than failing it, since the write has no log to record itself
/// on until the context exists.
fn open_registry(
    run_dir: &Path,
    clock: &dyn yunta_core::Clock,
) -> (
    Option<Arc<crate::process_registry::ProcessRegistry>>,
    Option<std::io::Error>,
) {
    match crate::process_registry::ProcessRegistry::create(run_dir, Pid::current(), clock.now()) {
        Ok(registry) => (Some(Arc::new(registry)), None),
        Err(e) => (None, Some(e)),
    }
}

/// The per-run MCP host, built beside the context it outlives. It outlives
/// every borrow of this invocation, so it owns a clone of each value it reads:
/// the one injected clock and the one display surface reach the listener's
/// own event appends through those clones.
fn mcp_host(
    env: &RunEnv<'_>,
    redactor: &yunta_core::Redactor,
    memo: &Arc<Memo>,
    pool: &Arc<crate::worktree::CheckoutPool>,
    registry: &Option<Arc<crate::process_registry::ProcessRegistry>>,
    subprocess_vars: &[(String, String)],
) -> Arc<crate::run_tools::RunToolsHost> {
    // One host per execute_run invocation, shared by every session
    // listener; each of them reads and writes through the host's own
    // clone of the log handle.
    Arc::new(crate::run_tools::RunToolsHost::new(
        &env.manifest.workflow,
        crate::run_tools::HostOf {
            storage: env.storage.clone(),
            run_id: env.run_id.clone(),
            clock: env.clock.clone(),
            observer: env.observer.clone(),
            run_dir: env.run_dir.to_path_buf(),
            max_artifact_bytes: env
                .manifest
                .config
                .limits
                .as_ref()
                .and_then(|limits| limits.max_artifact_bytes),
            redactor: redactor.clone(),
            memo: memo.clone(),
            pool: pool.clone(),
            process_registry: registry.clone(),
            subprocess_vars: subprocess_vars.to_vec(),
            search_path: env
                .ambient
                .map(|ambient| ambient.path.clone())
                .unwrap_or_default(),
            environment: crate::process::execution_environment(env.ambient),
            worktree: env.worktree.to_path_buf(),
            suite: crate::run::baseline::suite(env.manifest).and_then(yunta_core::Suite::of),
            formatters: crate::run_tools::Formatter::of(&env.manifest.config),
        },
    ))
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
