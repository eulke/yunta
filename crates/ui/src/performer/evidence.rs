//! See [`super`]. What a run leaves, each as its command derives and writes it: the
//! receipt beside the run, its evidence verified, and its page for a browser.

use yunta_api::{Error, Project, Remarks};

use crate::{EvidenceOperation, EvidenceOutcome, Outcome};

/// What `work` comes to, through the API, what it says said through `remarks`.
pub(super) async fn performed(
    project: &Project,
    work: EvidenceOperation,
    remarks: &dyn Remarks,
) -> Result<Outcome, Error> {
    let outcome = match work {
        EvidenceOperation::WriteReceipt { run } => {
            let receipt = project.run(&run).await?.receipt()?;
            EvidenceOutcome::Receipt(Box::new(receipt))
        }
        EvidenceOperation::Verify { run } => {
            let verification = project.verification(&run).await?;
            let checks = verification.checks();
            EvidenceOutcome::Verified { run, checks }
        }
        EvidenceOperation::WritePageForBrowser { run } => {
            let page = project.run(&run).await?.page_for_a_browser(remarks).await?;
            EvidenceOutcome::PageWritten { page }
        }
    };
    Ok(Outcome::Evidence(outcome))
}
