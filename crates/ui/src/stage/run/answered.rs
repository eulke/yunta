//! See [`super`]. What an answer to the work a run's stage asked for does: an answer given on
//! its prompt in place, once recorded, noted with what the run does now; what a pull request's
//! forge and a receipt's store answered is kept beside what it was asked for; and what an act
//! refused or failed on is noted until the next key, a close that left its run open among it,
//! an answer given in place handed back to its prompt. What the run does after an act is the
//! state's following to bring.

use super::frame::Notice;
use super::RunStage;
use crate::stage::Want;
use yunta_render::refusal::Refused;

use crate::{EvidenceOperation, EvidenceOutcome, Operation, Outcome};

impl RunStage {
    /// What an answer to the work `asked`, which the stage asked for, does: an answer given on
    /// its prompt in place,
    /// recorded on this run's log, noted with what the run does now; what the forge and the
    /// store answered, kept; what refused or failed is said: in the receipt's place where its
    /// chain could not be verified, as a note otherwise, an answer given in place handed back
    /// to its prompt.
    pub(crate) fn answered(&mut self, asked: &Operation, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Failed(said) if verifies_a_chain(asked) => self.chain_read(Err(said)),
            Outcome::Failed(said) => self.noticed(asked, Notice::Failed(said)),
            Outcome::PullRequest(polled) => self.pull_request_read(polled),
            Outcome::Evidence(EvidenceOutcome::Verified { chain, .. }) => {
                self.chain_read(Ok(chain))
            }
            Outcome::Decided { run: recorded } | Outcome::Answered { run: recorded }
                if recorded == self.run =>
            {
                self.recorded_here();
                Vec::new()
            }
            Outcome::Refused(refused) => self.noticed(asked, Notice::Refused(refused.to_string())),
            Outcome::Closed {
                refused, failed, ..
            } => {
                self.notice = unclosed(&refused, failed.as_ref()).or(self.notice.take());
                Vec::new()
            }
            Outcome::Repository(_)
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::Started { .. }
            | Outcome::Resumed { .. }
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Cancelled { .. }
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Doctor(_)
            | Outcome::Collected(_)
            | Outcome::Detected { .. }
            | Outcome::SetupPack { .. }
            | Outcome::Initialized { .. }
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// A decision on the run, or a round's answers, recorded here: what the run does now
    /// noted, where the answer was given on its prompt in place.
    fn recorded_here(&mut self) {
        if let Some(said) = self.answer_recorded() {
            self.notice = Some(Notice::Recorded(said));
        }
    }

    /// What an act no standing stage asked came to, noted on the run where it did not do what
    /// was asked; no prompt in place takes it, as none here asked it.
    pub(crate) fn noted(&mut self, outcome: &Outcome) {
        let notice = match outcome {
            Outcome::Refused(refused) => Some(Notice::Refused(refused.to_string())),
            Outcome::Failed(said) => Some(Notice::Failed(said.clone())),
            Outcome::Closed {
                refused, failed, ..
            } => unclosed(refused, failed.as_ref()),
            _ => None,
        };
        self.notice = notice.or(self.notice.take());
    }

    /// `notice`, what the work `asked` refused or failed on, noted where the run is; where that
    /// work was recording an answer given on the prompt in place, an option or a round's
    /// answers, the answer handed back to it.
    fn noticed(&mut self, asked: &Operation, notice: Notice) -> Vec<Want> {
        if let Operation::Decide { .. } | Operation::Answer { .. } = asked {
            self.answer_unrecorded();
        }
        self.notice = Some(notice);
        Vec::new()
    }
}

/// What a close came to where it left a run open: what failed below Yunta and stopped it,
/// else why it refused the first run it left; none where it closed every run it was asked to.
fn unclosed(refused: &[Refused], failed: Option<&String>) -> Option<Notice> {
    match (failed, refused.first()) {
        (Some(failed), _) => Some(Notice::Failed(failed.clone())),
        (None, Some(refused)) => Some(Notice::Refused(refused.to_string())),
        (None, None) => None,
    }
}

/// Whether the work `asked` verifies the run's event chain.
fn verifies_a_chain(asked: &Operation) -> bool {
    matches!(asked, Operation::Evidence(EvidenceOperation::Verify { .. }))
}
