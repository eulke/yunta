//! See [`super`]. What an answer to the catalog's work does: the workflows read, the pointer
//! kept on the one it stood on; a check, a workflow written or the cases' tally, on the page
//! that waited on it; the packs' own on the packs; and what refused or failed, said where the
//! work that came to it was asked, a page waiting on other work left standing.

use yunta_render::page::Catalog;

use super::{CatalogStage, Shown};
use crate::stage::Want;
use crate::{Operation, Outcome, WorkflowOutcome};

impl CatalogStage {
    /// What an answer to the stage's work `asked` does: the workflows read, the pointer
    /// kept on the one it stood on while it is still offered; a check, a workflow
    /// written or the cases' tally, on the page that waited on it; what refused or
    /// failed, said where the work that came to it was asked (`unanswered`).
    pub(crate) fn answered(&mut self, asked: &Operation, outcome: Outcome) -> Vec<Want> {
        let standing = self.standing();
        let wants = self.answered_where_it_stands(asked, outcome);
        self.rewound_unless(standing);
        wants
    }

    /// What an answer to `asked` does where the catalog stands.
    fn answered_where_it_stands(&mut self, asked: &Operation, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Catalog(catalog) => {
                self.read(catalog);
                Vec::new()
            }
            Outcome::Workflow(outcome) => {
                self.workflow_answered(outcome);
                Vec::new()
            }
            Outcome::Pack(outcome) => match &mut self.shown {
                Shown::Packs(packs) => {
                    let next = packs.answered(outcome);
                    self.went(next)
                }
                _ => Vec::new(),
            },
            Outcome::Refused(refused) => self.unanswered(asked, refused.to_string()),
            Outcome::Failed(said) => self.unanswered(asked, said),
            Outcome::Repository(_)
            | Outcome::PullRequest(_)
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Doctor(_)
            | Outcome::Collected(_)
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Started { .. }
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Evidence(_)
            | Outcome::Detected { .. }
            | Outcome::SetupPack { .. }
            | Outcome::Initialized { .. }
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// What work on a workflow came to, on the page that waited on it: a check, a
    /// new workflow written, or the cases' tally.
    fn workflow_answered(&mut self, outcome: WorkflowOutcome) {
        match (outcome, &mut self.shown) {
            (WorkflowOutcome::Checked(check), Shown::Checking { name }) => {
                let name = name.clone();
                self.shown = Shown::Checked { name, check };
            }
            (WorkflowOutcome::Written(written), Shown::New(asked)) => asked.answered(written),
            (WorkflowOutcome::Tested(tested), Shown::Cases(cases)) => cases.answered(tested),
            (WorkflowOutcome::Checked(_), _)
            | (WorkflowOutcome::Written(_), _)
            | (WorkflowOutcome::Tested(_), _) => {}
        }
    }

    /// The workflows `catalog` offers, read: the pointer stays on the one it stood
    /// on while it is offered, and goes to the first otherwise.
    fn read(&mut self, catalog: Box<Catalog>) {
        self.reading = false;
        let names = catalog.names();
        let kept = self
            .pointed
            .as_deref()
            .is_some_and(|pointed| names.contains(&pointed));
        if !kept {
            self.pointed = names.first().map(|name| name.to_string());
        }
        self.catalog = Some(catalog);
    }

    /// `said`, what the work `asked` refused or failed on, said: over the packs where they
    /// stand and it was theirs; over the workflows, the page standing, where it was their own
    /// reading, which a page shown in their place does not wait on; over the workflows
    /// otherwise, which the page that waited on it gives way to.
    fn unanswered(&mut self, asked: &Operation, said: String) -> Vec<Want> {
        match (&mut self.shown, asked) {
            (Shown::Packs(packs), Operation::Pack(work)) => packs.unanswered(work, said),
            (_, Operation::ReadCatalog) => {
                self.reading = false;
                self.said = Some(said);
            }
            (
                Shown::Catalog
                | Shown::Checking { .. }
                | Shown::Checked { .. }
                | Shown::New(_)
                | Shown::Cases(_)
                | Shown::Packs(_),
                _,
            ) => {
                self.said = Some(said);
                self.shown = Shown::Catalog;
            }
        }
        Vec::new()
    }
}
