//! `yunta resume <run_id>`: reload the frozen manifest from run.dir and
//! drive the run forward with the same `execute_run` that started it —
//! the log decides what remains.
//!
//! run.dir is looked up in search order (current config's runs root,
//! then the default) — and once the manifest is open, the worktree
//! comes from its *frozen* paths, so a `paths.*` change between `run`
//! and `resume` never loses the run.

use std::process::ExitCode;

use yunta_core::{Isolation, Manifest, RunId, SystemClock, SystemIdSource};
use yunta_engine::{RunEnv, RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_storage::AsyncStorage;

use crate::load_yaml;
use crate::project;

pub async fn resume(run_id: &RunId) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let project = match project::resolve(&cwd) {
        Ok(project) => project,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some(run_dir) = project::find_run_dir(&project, run_id.as_str()) else {
        eprintln!(
            "error: no run `{run_id}` under {} (or the default state root) — nothing to              resume; a run created under roots no longer in any config layer needs              YUNTA_HOME pointing there",
            project.runs_root.display()
        );
        return ExitCode::FAILURE;
    };
    let manifest_path = run_dir.join("manifest.yaml");
    let manifest: Manifest = match load_yaml(&manifest_path, "run manifest") {
        Ok(manifest) => manifest,
        Err(code) => return code,
    };

    // The manifest's own frozen config, not the project's current one:
    // a run never re-reads config after it's created.
    let adapters = super::real_adapters(&manifest.config);
    if let Err(code) = super::refuse_unrunnable(&manifest.workflow, &adapters) {
        return code;
    }

    let storage = match AsyncStorage::open(&project.storage_path).await {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // The worktree (or the checkout itself, for `none`) was already
    // prepared by the `run` that created this run — resume finds it by
    // the same rule, it never prepares a fresh one.
    // The frozen roots win; a pre-freeze manifest (no `paths:`) falls
    // back to the current config, exactly the old behavior.
    let worktrees_root = manifest
        .paths
        .as_ref()
        .map(|paths| paths.worktrees_root.clone())
        .unwrap_or_else(|| project.worktrees_root.clone());
    let worktree = match manifest.isolation {
        Isolation::Worktree => worktrees_root.join(run_id.as_str()),
        Isolation::None => cwd.clone(),
    };

    // Same rule as `adapters` above: the manifest's own frozen config,
    // never the project's current one.
    let forge = super::real_forge(&manifest.config);
    let root_cancel = super::cancel_on_ctrl_c();
    let outcome = yunta_engine::execute_run(RunEnv {
        run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &worktree,
        adapters: &adapters,
        storage: &storage,
        clock: std::sync::Arc::new(SystemClock),
        ids: &SystemIdSource,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &crate::human_interaction::ConsoleInteraction,
        forge: forge.as_deref(),
        cancel: Some(&root_cancel),
        adapter_override: None,
    })
    .await;

    match outcome {
        Ok(report) => {
            let (run_id, manifest, _worktree, report) = match super::promote::drive_promotions(
                &super::promote::PromotionEnv {
                    cwd: &cwd,
                    project: &project,
                    storage: &storage,
                    ids: &SystemIdSource,
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
            {
                Ok(chained) => chained,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            // A user cancellation also releases `none`'s lock — the
            // engine process is exiting, and a Ctrl-C is designed to
            // leave nothing held.
            if matches!(report.terminal, RunTerminal::Finished) || root_cancel.is_cancelled() {
                if let Err(e) = yunta_engine::release_worktree(&cwd, manifest.isolation).await {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            }
            super::report_outcome(run_id.as_str(), &report)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
