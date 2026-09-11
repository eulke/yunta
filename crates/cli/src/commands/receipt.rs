//! `yunta receipt <run_id>`: gathers the run's manifest, event log and
//! hash-chain verification off disk — the only
//! IO this command does — and hands them to `yunta_engine::build_receipt`
//! for the actual derivation. Writes both formats to the run's own
//! `run.dir` (`receipt.md`, `receipt.json`) so a later node (e.g. a `pr`
//! bash step doing `gh pr create --body-file receipt.md`) can pick them
//! up, and prints one of them to stdout for a human running the command
//! directly.

use std::path::{Path, PathBuf};

use yunta_core::{Manifest, RunId};
use yunta_engine::{
    build_receipt, render_receipt_json, render_receipt_markdown, EventChainStatus, Receipt,
};
use yunta_storage::ChainVerification;

use crate::context::Context;
use crate::error::{CliError, Outcome};

pub fn receipt(run_id: &RunId, json: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let (run_dir, receipt) = gathered(&ctx, run_id)?;

    let markdown = render_receipt_markdown(&receipt);
    let json_text = render_receipt_json(&receipt)
        .map_err(|e| CliError::msg(format!("could not render receipt JSON: {e}")))?;

    // Both formats land beside the run whichever one is asked for, so a
    // later node picks up the one it needs without running this again.
    save(&run_dir.join("receipt.md"), &markdown)?;
    save(&run_dir.join("receipt.json"), &json_text)?;

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

/// The run's receipt and the directory it belongs beside, derived from
/// what this command reads off disk: the log, the manifest the run
/// froze, and the hash chain's own verdict.
fn gathered(ctx: &Context, run_id: &RunId) -> Result<(PathBuf, Receipt), CliError> {
    let storage = ctx.storage()?;
    let events = storage.events_for_run(run_id)?;
    if events.is_empty() {
        return Err(CliError::msg(format!(
            "no run `{run_id}` in {}",
            ctx.project.storage_path.display()
        )));
    }

    let run_dir = ctx
        .project
        .run_dir(run_id.as_str())
        .unwrap_or_else(|| ctx.project.runs_root.join(run_id.as_str()));
    let manifest: Manifest = crate::load_yaml(&run_dir.join("manifest.yaml"), "run manifest")?;

    let chain = match storage.verify_chain(run_id)? {
        ChainVerification::Intact { events } => EventChainStatus::Intact { events },
        ChainVerification::Broken { seq, detail } => EventChainStatus::Broken { seq, detail },
    };

    let receipt = build_receipt(run_id, &manifest, &events, chain).map_err(|e| {
        // The engine says which state the run is in; which command shows
        // a reader where that run stands is this border's vocabulary.
        CliError::msg(format!(
            "{e}; `{}` shows where it is",
            super::advice::status(run_id)
        ))
    })?;
    Ok((run_dir, receipt))
}

/// Writes one rendering beside the run, naming the file it could not
/// write.
fn save(path: &Path, contents: &str) -> Result<(), CliError> {
    std::fs::write(path, contents).map_err(|source| CliError::io("write", path.display(), source))
}
