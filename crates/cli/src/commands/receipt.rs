//! `yunta receipt <run_id>`: gathers the run's manifest, event log and
//! hash-chain verification off disk — the only
//! IO this command does — and hands them to `yunta_engine::build_receipt`
//! for the actual derivation. Writes both formats to the run's own
//! `run.dir` (`receipt.md`, `receipt.json`) so a later node (e.g. a `pr`
//! bash step doing `gh pr create --body-file receipt.md`) can pick them
//! up, and prints one of them to stdout for a human running the command
//! directly.

use yunta_core::{Manifest, RunId};
use yunta_engine::{build_receipt, render_receipt_json, render_receipt_markdown, EventChainStatus};
use yunta_storage::{ChainVerification, Storage};

use crate::error::{CliError, Outcome};

pub fn receipt(run_id: &RunId, json: bool) -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;
    let project = crate::project::resolve(&cwd)?;
    let storage = Storage::open(&project.storage_path)?;
    let events = storage.events_for_run(run_id)?;
    if events.is_empty() {
        return Err(CliError::msg(format!(
            "no run `{run_id}` in {}",
            project.storage_path.display()
        )));
    }

    let run_dir = crate::project::find_run_dir(&project, run_id.as_str())
        .unwrap_or_else(|| project.runs_root.join(run_id.as_str()));
    let manifest: Manifest = crate::load_yaml(&run_dir.join("manifest.yaml"), "run manifest")?;

    let chain = match storage.verify_chain(run_id)? {
        ChainVerification::Intact { events } => EventChainStatus::Intact { events },
        ChainVerification::Broken { seq, detail } => EventChainStatus::Broken { seq, detail },
    };

    let receipt = build_receipt(run_id, &manifest, &events, chain)
        .map_err(|e| CliError::msg(e.to_string()))?;

    let markdown = render_receipt_markdown(&receipt);
    let json_text = render_receipt_json(&receipt)
        .map_err(|e| CliError::msg(format!("could not render receipt JSON: {e}")))?;

    std::fs::write(run_dir.join("receipt.md"), &markdown).map_err(|e| {
        CliError::msg(format!(
            "could not write {}: {e}",
            run_dir.join("receipt.md").display()
        ))
    })?;
    std::fs::write(run_dir.join("receipt.json"), &json_text).map_err(|e| {
        CliError::msg(format!(
            "could not write {}: {e}",
            run_dir.join("receipt.json").display()
        ))
    })?;

    if json {
        println!("{json_text}");
    } else {
        // `markdown` already ends with its own trailing newline —
        // `print!`, not `println!`, so stdout matches `receipt.md` byte
        // for byte instead of gaining a second one.
        print!("{markdown}");
    }
    Ok(Outcome::Success)
}
