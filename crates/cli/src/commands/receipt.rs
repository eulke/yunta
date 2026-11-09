//! `yunta receipt <run_id>`: the run's receipt, which the API derives and
//! writes beside the run in both formats (`receipt.md`, `receipt.json`) so a
//! later node (e.g. a `pr` bash step doing `gh pr create --body-file
//! receipt.md`) can pick them up, printed for whoever ran the command.

use surco::look::Look;
use yunta_core::receipt::Receipt;
use yunta_core::RunId;
use yunta_render::page::{self, Page};

use crate::error::CliError;
use crate::output::Outcome;
use crate::output::{emit, Answer, Mode};
use crate::shell::Shell;

pub async fn receipt(
    shell: &Shell,
    run_id: &RunId,
    output: Mode,
    look: &Look,
) -> Result<Outcome, CliError> {
    let project = shell.workspace().project()?;
    let receipt = project.run(run_id).await?.receipt()?;
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
    fn page(&self) -> impl Page + '_ {
        page::Receipt {
            receipt: &self.receipt,
        }
    }

    fn outcome(&self) -> Outcome {
        Outcome::Success
    }
}
