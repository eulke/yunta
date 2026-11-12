//! See [`super`]. A run's receipt: derived from its log, the manifest it froze
//! and its hash chain's verdict, and written beside the run in both formats,
//! so a later node — a `pr` step handing a forge `receipt.md` — picks up the
//! one it needs without asking for it again.

use std::path::{Path, PathBuf};

use surco::surface::{Markdown, Surface};
use yunta_core::receipt::{EventChainStatus, Receipt};
use yunta_render::page::{self, Page};
use yunta_render::refusal::Refused;
use yunta_storage::{ChainVerification, Storage};

use super::OpenedRun;
use crate::{Error, Failure};

impl OpenedRun<'_> {
    /// The run's receipt, written beside the run as `receipt.md` and
    /// `receipt.json`: refused for a run that has not finished, which has no
    /// closing figures to certify.
    pub fn receipt(&self) -> Result<Receipt, Error> {
        let storage = Storage::open(&self.project.storage_path).map_err(Failure::new)?;
        let chain = match storage.verify_chain(&self.id).map_err(Failure::new)? {
            ChainVerification::Intact { events } => EventChainStatus::Intact { events },
            ChainVerification::Broken { seq, detail } => EventChainStatus::Broken { seq, detail },
        };
        let receipt =
            yunta_replay::receipt::closed(&self.id, &self.manifest.doc, &self.events, chain)
                .map_err(Refused::NoReceipt)?;
        // A file a forge is handed as a pull request's body: drawn with the
        // Markdown look whatever the reader's terminal reads, as the forge
        // draws it.
        let markdown =
            Markdown.draw(&page::Receipt { receipt: &receipt }.document(&Markdown.look()));
        let json = serde_json::to_string_pretty(&receipt)
            .map_err(|source| Failure::new(ReceiptUnwritten::Json(source)))?;
        write(&self.dir.join("receipt.md"), &markdown)?;
        write(&self.dir.join("receipt.json"), &json)?;
        Ok(receipt)
    }
}

/// A receipt that could not be written: its JSON, or one of its files.
#[derive(Debug, thiserror::Error)]
enum ReceiptUnwritten {
    #[error("could not render receipt JSON: {0}")]
    Json(serde_json::Error),
    #[error("failed to write {}: {source}", path.display())]
    File {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Writes one format of the receipt beside the run, naming the file it could
/// not write.
fn write(path: &Path, contents: &str) -> Result<(), Failure> {
    std::fs::write(path, contents).map_err(|source| {
        Failure::new(ReceiptUnwritten::File {
            path: path.to_path_buf(),
            source,
        })
    })
}
