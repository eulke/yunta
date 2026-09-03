//! `yunta resume <run_id>`: reload the frozen manifest from run.dir and
//! drive the run forward with the same `execute_run` that started it —
//! the log decides what remains.
//!
//! run.dir is looked up in search order (current config's runs root,
//! then the default) — and once the manifest is open, the worktree
//! comes from its *frozen* paths, so a `paths.*` change between `run`
//! and `resume` never loses the run.

use yunta_core::{Isolation, Manifest, RunId};
use yunta_engine::{RunEnv, RunTerminal, DEFAULT_MAX_RETRIES};

use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::load_yaml;

pub async fn resume(run_id: &RunId) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let Some(run_dir) = ctx.project.run_dir(run_id.as_str()) else {
        return Err(CliError::msg(format!(
            "no run `{run_id}` under {} (or the default state root) — nothing to              resume; a run created under roots no longer in any config layer needs              YUNTA_HOME pointing there",
            ctx.project.runs_root.display()
        )));
    };
    let manifest_path = run_dir.join("manifest.yaml");
    let manifest: Manifest = load_yaml(&manifest_path, "run manifest")?;

    // The manifest's own frozen config, not the project's current one:
    // a run never re-reads config after it's created.
    let adapters = super::real_adapters(&manifest.config);
    super::refuse_unrunnable(&manifest.workflow, &adapters)?;

    let storage = ctx.async_storage().await?;

    // The worktree (or the checkout itself, for `none`) was already
    // prepared by the `run` that created this run — resume finds it by
    // the same rule (the manifest's frozen root), never a fresh one.
    let worktree = match manifest.isolation {
        Isolation::Worktree => ctx
            .project
            .worktrees_root_for(&manifest)
            .join(run_id.as_str()),
        Isolation::None => ctx.cwd.clone(),
    };

    // Same rule as `adapters` above: the manifest's own frozen config,
    // never the project's current one.
    let forge = super::real_forge(&manifest.config);
    let root_cancel = super::cancel_on_ctrl_c();
    let report = yunta_engine::execute_run(RunEnv {
        run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &worktree,
        adapters: &adapters,
        storage: &storage,
        clock: std::sync::Arc::new(ctx.clock),
        ids: &ctx.ids,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &crate::human_interaction::ConsoleInteraction,
        forge: forge.as_deref(),
        cancel: Some(&root_cancel),
        adapter_override: None,
    })
    .await?;

    let (run_id, manifest, _worktree, report) = super::promote::drive_promotions(
        &super::promote::PromotionEnv {
            cwd: &ctx.cwd,
            project: &ctx.project,
            storage: &storage,
            ids: &ctx.ids,
            adapters: &adapters,
            forge: forge.as_deref(),
            cancel: Some(&root_cancel),
        },
        run_id.clone(),
        manifest,
        worktree,
        report,
    )
    .await
    .map_err(CliError::msg)?;
    // A user cancellation also releases `none`'s lock — the engine
    // process is exiting, and a Ctrl-C is designed to leave nothing held.
    if matches!(report.terminal, RunTerminal::Finished) || root_cancel.is_cancelled() {
        yunta_engine::release_worktree(&ctx.cwd, manifest.isolation).await?;
    }
    Ok(super::report_outcome(run_id.as_str(), &report))
}
