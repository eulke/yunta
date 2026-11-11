//! See [`super`]. What a run leaves, opened from its stage: its receipt, written
//! beside it and shown as `yunta receipt` prints it; its log and the objects it names
//! verified, as `yunta verify` says them; or its page written where a browser reads
//! it, then shown by the surface ([`Want::Show`]): a terminal hands it to the
//! system's opener, the desktop to a window. A receipt or a check taller than what
//! shows of it scrolls. Escape goes back to the run.

use std::path::PathBuf;

use surco::components::{Check, Laid, Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::receipt::Receipt;
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_render::page::{self, Opened, Page, Verified};

use super::{Act, Want};
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{EvidenceOperation, EvidenceOutcome, Operation, Outcome};

/// What a run leaves, one of it asked for.
#[derive(Debug, Clone)]
pub struct EvidenceStage {
    run: RunId,
    asked: Evidence,
    shown: Shown,
    /// Where what stands is scrolled to.
    scroll: Scroll,
    /// What refused or failed, where something did.
    said: Option<String>,
}

/// What of a run's evidence is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// Its receipt.
    Receipt,
    /// Its log and the objects it names, verified.
    Verification,
    /// Its page, where a browser reads it.
    PageForABrowser,
}

/// What stands on the screen.
#[derive(Debug, Clone)]
enum Shown {
    /// What was asked for, being derived.
    Deriving,
    /// The receipt written.
    Receipt(Box<Receipt>),
    /// Each check of the run's evidence.
    Verified(Vec<Check>),
    /// Where the page for a browser was written.
    Opened(PathBuf),
}

impl EvidenceStage {
    /// `asked` of `run`, yet to be derived.
    pub(crate) fn of(run: RunId, asked: Evidence) -> Self {
        EvidenceStage {
            run,
            asked,
            shown: Shown::Deriving,
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What the stage asks for as it opens: what was asked for, derived.
    pub(crate) fn opening(&self) -> Vec<Want> {
        let run = self.run.clone();
        let work = match self.asked {
            Evidence::Receipt => EvidenceOperation::WriteReceipt { run },
            Evidence::Verification => EvidenceOperation::Verify { run },
            Evidence::PageForABrowser => EvidenceOperation::WritePageForBrowser { run },
        };
        vec![Want::Work(Operation::Evidence(work))]
    }

    /// What was asked for, being derived or as its command says it, and what
    /// refused or failed; with the keys.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let mut doc = match &self.shown {
            Shown::Deriving => Doc::new().with(Note(self.deriving().to_string())),
            Shown::Receipt(receipt) => page::Receipt { receipt }.document(look),
            Shown::Verified(checks) => Verified {
                run_id: &self.run,
                checks,
            }
            .document(look),
            Shown::Opened(page) => {
                let page = page.display().to_string();
                Opened {
                    page: &page,
                    opened: None,
                }
                .document(look)
            }
        };
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut screen = Screen::new(Region::new(doc).in_sight(self.scroll.sight()));
        screen.trail = vec![
            yunta_render::screen::YUNTA.to_string(),
            self.run.handle().to_string(),
            self.trail().to_string(),
        ];
        let legend = match self.shown {
            Shown::Opened(_) => yunta_render::screen::opened_legend(),
            Shown::Deriving | Shown::Receipt(_) | Shown::Verified(_) => {
                yunta_render::screen::over_the_run_legend()
            }
        };
        screen.legend = match self.scroll.scrolls() {
            true => yunta_render::screen::scrolling(legend),
            false => legend,
        };
        screen
    }

    /// What stands while what was asked for is derived.
    fn deriving(&self) -> &'static str {
        match self.asked {
            Evidence::Receipt => yunta_render::screen::WRITING_THE_RECEIPT,
            Evidence::Verification => yunta_render::screen::VERIFYING,
            Evidence::PageForABrowser => yunta_render::screen::WRITING_THE_PAGE,
        }
    }

    /// Where the trail goes on past the run.
    fn trail(&self) -> &'static str {
        match self.asked {
            Evidence::Receipt => yunta_render::screen::RECEIPT,
            Evidence::Verification => yunta_render::screen::VERIFICATION,
            Evidence::PageForABrowser => yunta_render::screen::PAGE,
        }
    }

    /// What `stroke` asks for here: Enter shows the page for a browser again,
    /// Escape goes back to the run, `q` or Ctrl-C leaves, and the keys that scroll
    /// scroll what stands.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        match (stroke, &self.shown) {
            (Stroke::Enter, Shown::Opened(page)) => vec![Want::Show(page.clone())],
            (Stroke::Decline, _) => vec![Want::Back],
            (Stroke::Insert('q') | Stroke::Interrupt, _) => vec![Want::Leave],
            (other, _) => {
                self.scroll.pressed(other);
                Vec::new()
            }
        }
    }

    /// What the stage offers, with the key that takes it here: the page for a browser
    /// shown again, once it is written.
    pub(crate) fn offered(&self) -> Vec<Act> {
        match self.shown {
            Shown::Opened(_) => vec![Act::by(
                Stroke::Enter,
                NextStep::Open {
                    run: self.run.clone(),
                },
            )],
            Shown::Deriving | Shown::Receipt(_) | Shown::Verified(_) => Vec::new(),
        }
    }

    /// What a surface laid out of what stands, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }

    /// What an answer to the stage's work does: what was asked for stands, and a
    /// page for a browser is shown as soon as it is written; what refused or failed
    /// is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Evidence(EvidenceOutcome::Receipt(receipt)) => {
                self.shown = Shown::Receipt(receipt);
                Vec::new()
            }
            Outcome::Evidence(EvidenceOutcome::Verified { checks, .. }) => {
                self.shown = Shown::Verified(checks);
                Vec::new()
            }
            Outcome::Evidence(EvidenceOutcome::PageWritten { page }) => {
                self.shown = Shown::Opened(page.clone());
                vec![Want::Show(page)]
            }
            Outcome::Refused(refused) => {
                self.said = Some(refused.to_string());
                Vec::new()
            }
            Outcome::Failed(said) => {
                self.said = Some(said);
                Vec::new()
            }
            Outcome::Repository(_)
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Field { .. }
            | Outcome::Followed
            | Outcome::Page(_)
            | Outcome::NodePage(_)
            | Outcome::Questions(_)
            | Outcome::Decision(_)
            | Outcome::Stats(_)
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
            | Outcome::Detected { .. }
            | Outcome::Initialized(_)
            | Outcome::Stopped => Vec::new(),
        }
    }
}
