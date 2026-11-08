//! `yunta receipt <run_id>`: gathers the run's manifest, event log and
//! hash-chain verification off disk — the only
//! IO this command does — and hands them to `yunta_replay::receipt::closed`
//! for the actual derivation. Writes both formats to the run's own
//! `run.dir` (`receipt.md`, `receipt.json`) so a later node (e.g. a `pr`
//! bash step doing `gh pr create --body-file receipt.md`) can pick them
//! up, and prints one of them to stdout for a human running the command
//! directly.

use std::path::{Path, PathBuf};

use surco::doc::Doc;
use surco::look::Look;
use yunta_core::receipt::{EventChainStatus, Receipt};
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_render::command;
use yunta_replay::receipt;
use yunta_storage::ChainVerification;

use crate::context::Context;
use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};

pub async fn receipt(run_id: &RunId, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let (run_dir, receipt) = gathered(&ctx, run_id).await?;

    let markdown = {
        use surco::surface::Surface;
        surco::surface::Markdown.draw(&yunta_render::receipt::document(&receipt, look.glyphs))
    };
    let json_text = serde_json::to_string_pretty(&receipt)
        .map_err(|e| CliError::msg(format!("could not render receipt JSON: {e}")))?;

    // Both formats land beside the run whichever one is asked for, so a
    // later node picks up the one it needs without running this again.
    save(&run_dir.join("receipt.md"), &markdown)?;
    save(&run_dir.join("receipt.json"), &json_text)?;

    emit(output, &ReceiptAnswer { receipt }, look)
}

/// The receipt as an answer. Its JSON is the receipt's own document, and its
/// page is the same document laid out for this terminal rather than written
/// as Markdown.
#[derive(serde::Serialize)]
#[serde(transparent)]
struct ReceiptAnswer {
    receipt: Receipt,
}

impl Answer for ReceiptAnswer {
    fn document(&self, look: &Look) -> Doc {
        yunta_render::receipt::document(&self.receipt, look.glyphs)
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}

/// The run's receipt and the directory it belongs beside, derived from
/// what this command reads off disk: the log, the manifest the run
/// froze, and the hash chain's own verdict.
async fn gathered(ctx: &Context, run_id: &RunId) -> Result<(PathBuf, Receipt), CliError> {
    let storage = ctx.storage()?;
    let open = ctx.open_run(run_id).await?;
    let (run_dir, events, manifest) = (open.run_dir, open.events, open.manifest.doc);

    let chain = match storage.verify_chain(run_id)? {
        ChainVerification::Intact { events } => EventChainStatus::Intact { events },
        ChainVerification::Broken { seq, detail } => EventChainStatus::Broken { seq, detail },
    };

    let receipt = receipt::closed(run_id, &manifest, &events, chain).map_err(|e| {
        // The engine says which state the run is in; which command shows
        // a reader where that run stands is this border's vocabulary.
        CliError::msg(format!(
            "{e}; `{}` shows where it is",
            command::line(&NextStep::status(run_id))
        ))
    })?;
    Ok((run_dir, receipt))
}

/// Writes one rendering beside the run, naming the file it could not
/// write.
pub(super) fn save(path: &Path, contents: &str) -> Result<(), CliError> {
    std::fs::write(path, contents).map_err(|source| CliError::io("write", path.display(), source))
}
