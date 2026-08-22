//! `yunta receipt <run_id>`: gathers the run's manifest, event log and
//! hash-chain verification off disk — the only
//! IO this command does — and hands them to `yunta_engine::build_receipt`
//! for the actual derivation. Writes both formats to the run's own
//! `run.dir` (`receipt.md`, `receipt.json`) so a later node (e.g. a `pr`
//! bash step doing `gh pr create --body-file receipt.md`) can pick them
//! up, and prints one of them to stdout for a human running the command
//! directly.

use std::process::ExitCode;

use yunta_core::{Manifest, RunId};
use yunta_engine::{build_receipt, render_receipt_json, render_receipt_markdown, EventChainStatus};
use yunta_storage::{ChainVerification, Storage};

pub fn receipt(run_id: &str, json: bool) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let project = match crate::project::resolve(&cwd) {
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

    let run_id = RunId::from(run_id);
    let events = match storage.events_for_run(&run_id) {
        Ok(events) => events,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if events.is_empty() {
        eprintln!(
            "error: no run `{run_id}` in {}",
            project.storage_path.display()
        );
        return ExitCode::FAILURE;
    }

    let run_dir = crate::project::find_run_dir(&project, run_id.as_str())
        .unwrap_or_else(|| project.runs_root.join(run_id.as_str()));
    let manifest: Manifest = match crate::load_yaml(&run_dir.join("manifest.yaml"), "run manifest")
    {
        Ok(manifest) => manifest,
        Err(code) => return code,
    };

    let chain = match storage.verify_chain(&run_id) {
        Ok(ChainVerification::Intact { events }) => EventChainStatus::Intact { events },
        Ok(ChainVerification::Broken { seq, detail }) => EventChainStatus::Broken { seq, detail },
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let receipt = match build_receipt(&run_id, &manifest, &events, chain) {
        Ok(receipt) => receipt,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let markdown = render_receipt_markdown(&receipt);
    let json_text = match render_receipt_json(&receipt) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("error: could not render receipt JSON: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = std::fs::write(run_dir.join("receipt.md"), &markdown) {
        eprintln!(
            "error: could not write {}: {e}",
            run_dir.join("receipt.md").display()
        );
        return ExitCode::FAILURE;
    }
    if let Err(e) = std::fs::write(run_dir.join("receipt.json"), &json_text) {
        eprintln!(
            "error: could not write {}: {e}",
            run_dir.join("receipt.json").display()
        );
        return ExitCode::FAILURE;
    }

    if json {
        println!("{json_text}");
    } else {
        // `markdown` already ends with its own trailing newline —
        // `print!`, not `println!`, so stdout matches `receipt.md` byte
        // for byte instead of gaining a second one.
        print!("{markdown}");
    }
    ExitCode::SUCCESS
}
