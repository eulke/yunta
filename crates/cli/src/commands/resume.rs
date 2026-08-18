//! `yunta resume <run_id>` (T4.5 recorte, §8.1): reload the frozen
//! manifest from run.dir and drive the run forward with the same
//! `execute_run` that started it — the log decides what remains.
//!
//! M-0 cut: run.dir is looked up under the *current* config's runs root;
//! freezing resolved paths in the manifest so a later `paths.runs` change
//! cannot lose the run is T2.4, not built yet.

use std::process::ExitCode;

use yunta_core::{Manifest, RunId, SystemClock};
use yunta_engine::DEFAULT_MAX_RETRIES;
use yunta_storage::Storage;

use crate::load_yaml;
use crate::project;

pub async fn resume(run_id: &str) -> ExitCode {
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

    let run_id = RunId::from(run_id);
    let run_dir = project.runs_root.join(run_id.as_str());
    let manifest_path = run_dir.join("manifest.yaml");
    if !manifest_path.exists() {
        eprintln!(
            "error: no run `{run_id}` under {} — nothing to resume",
            project.runs_root.display()
        );
        return ExitCode::FAILURE;
    }
    let manifest: Manifest = match load_yaml(&manifest_path, "run manifest") {
        Ok(manifest) => manifest,
        Err(code) => return code,
    };

    let adapters = super::real_adapters();
    if let Err(code) = super::refuse_unrunnable(&manifest.workflow, &adapters) {
        return code;
    }

    let storage = match Storage::open(&project.storage_path) {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    match yunta_engine::execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &cwd,
        &adapters,
        &storage,
        &SystemClock,
        DEFAULT_MAX_RETRIES,
    )
    .await
    {
        Ok(report) => super::report_outcome(run_id.as_str(), &report),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
