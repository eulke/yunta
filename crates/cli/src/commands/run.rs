//! `yunta run <workflow>` (T7.1 parcial): resolve config, check, freeze
//! the manifest, create the run and execute it in place.
//!
//! M-0 cut: the working tree is the current repository itself (worktree
//! isolation is T4.2); the run id comes from the wall clock + pid — the
//! shell may use entropy, the engine never does.

use std::path::Path;
use std::process::ExitCode;

use yunta_core::{RunId, SystemClock, Workflow};
use yunta_engine::DEFAULT_MAX_RETRIES;
use yunta_storage::Storage;

use crate::load_yaml;
use crate::project;

pub async fn run(workflow_path: &Path) -> ExitCode {
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

    let workflow: Workflow = match load_yaml(workflow_path, "workflow") {
        Ok(w) => w,
        Err(code) => return code,
    };
    if let Err(code) = super::check_or_refuse(&workflow, &project.config) {
        return code;
    }
    let adapters = super::real_adapters();
    if let Err(code) = super::refuse_unrunnable(&workflow, &adapters) {
        return code;
    }

    let workflow_dir = workflow_path.parent().unwrap_or(Path::new("."));
    let manifest =
        match yunta_engine::build_manifest(&workflow, &project.config, workflow_dir, &cwd) {
            Ok(manifest) => manifest,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };

    let storage = match Storage::open(&project.storage_path) {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let run_id = RunId::from(format!(
        "run-{}-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"),
        std::process::id()
    ));
    let clock = SystemClock;

    let run_dir =
        match yunta_engine::create_run(&run_id, &manifest, &project.runs_root, &storage, &clock) {
            Ok(run_dir) => run_dir,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
    println!("run {run_id}: created at {}", run_dir.display());

    match yunta_engine::execute_run(
        &run_id,
        &manifest,
        &run_dir,
        &cwd,
        &adapters,
        &storage,
        &clock,
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
