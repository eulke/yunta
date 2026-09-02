//! `yunta verify <run_id>`: walks the run's event hash chain
//! recomputing every link from the bytes as persisted. Integrity and
//! order only — never authenticity, which is a separate layer.

use std::process::ExitCode;

use yunta_core::RunId;
use yunta_storage::{ChainVerification, Storage};

use crate::project;

pub fn verify(run_id: &RunId) -> ExitCode {
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
    let storage = match Storage::open(&project.storage_path) {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    match storage.verify_chain(run_id) {
        Ok(ChainVerification::Intact { events }) => {
            println!("run {run_id}: chain intact — {events} event(s) verified");
            ExitCode::SUCCESS
        }
        Ok(ChainVerification::Broken { seq, detail }) => {
            // A broken chain is the `broken` reading of the run: the
            // log can no longer be trusted from this point on.
            eprintln!("run {run_id}: chain BROKEN at seq {seq} — {detail}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
