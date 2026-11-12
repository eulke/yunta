//! See [`super`]. The repository's cases, run as `yunta test` runs them: each shown
//! as it ends, in the one column every case's name fits, what their runs say under
//! them, and the tally once every one ran. Escape stops them while they run, keeping
//! what ended, and goes back to the workflows once they stopped or ended.

use surco::components::{Legend, Note};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::Remark;
use yunta_render::page::{Page, RanCase, Tested, TestedCase};

use super::Next;
use crate::prompt::Stroke;
use crate::{News, Operation, WorkflowOperation};

/// The cases, as far as they ran.
#[derive(Debug, Clone)]
pub(super) struct CasesRun {
    /// Every case's name, once known: what sets the column the rows share.
    names: Vec<String>,
    /// The cases that ended, in the order they did, each once.
    ended: Vec<RanCase>,
    /// What their runs said while they were made.
    remarked: Vec<Remark>,
    /// How far they went.
    progress: Progress,
}

/// How far the cases went.
#[derive(Debug, Clone)]
enum Progress {
    Running,
    /// A person stopped them before every one ran.
    Stopped,
    /// Every one ran, and the tally.
    Ran(Tested),
}

impl CasesRun {
    /// Cases about to run, none ended yet.
    pub(super) fn new() -> Self {
        CasesRun {
            names: Vec::new(),
            ended: Vec::new(),
            remarked: Vec::new(),
            progress: Progress::Running,
        }
    }

    /// Each case that ended, as its row; what their runs said; and that the others
    /// run, that they were stopped, or the tally; with the keys.
    pub(super) fn screen(&self, look: &Look) -> (Doc, Legend) {
        let mut doc = Doc::new();
        for case in &self.ended {
            let row = TestedCase {
                names: &self.names,
                case: case.clone(),
            };
            doc = doc.with_all(row.document(look).into_blocks());
        }
        for remark in &self.remarked {
            doc = doc.with(Note(crate::remark::said(remark, look.glyphs).words));
        }
        match &self.progress {
            Progress::Running => {
                let running = Note(yunta_render::screen::RUNNING_THE_CASES.to_string());
                (doc.with(running), yunta_render::screen::cases_legend())
            }
            Progress::Stopped => {
                let stopped = Note(yunta_render::screen::CASES_STOPPED.to_string());
                (
                    doc.with(stopped),
                    yunta_render::screen::over_the_workflows_legend(),
                )
            }
            Progress::Ran(tested) => (
                doc.with_all(tested.document(look).into_blocks()),
                yunta_render::screen::over_the_workflows_legend(),
            ),
        }
    }

    /// What `stroke` does here: Escape stops the cases while they run, and goes back
    /// to the workflows, as Enter does, once they stopped or ran.
    pub(super) fn pressed(&mut self, stroke: Stroke) -> Next {
        match (stroke, &self.progress) {
            (Stroke::Insert('q') | Stroke::Interrupt, _) => Next::Leave,
            (Stroke::Decline, Progress::Running) => {
                self.progress = Progress::Stopped;
                Next::LetGo(Operation::Workflow(WorkflowOperation::RunCases))
            }
            (Stroke::Decline | Stroke::Enter, Progress::Stopped | Progress::Ran(_)) => Next::Done,
            _ => Next::Stays,
        }
    }

    /// What the cases running hand back: every name, and each case as it ends. A
    /// case ends once: one heard again is the one already shown.
    pub(super) fn heard(&mut self, news: News) {
        match news {
            News::CasesNamed(names) => self.names = names,
            News::Case(case) => {
                if !self.ended.iter().any(|ended| ended.name == case.name) {
                    self.ended.push(case);
                }
            }
            News::Following(_) | News::Run(_) => {}
        }
    }

    /// What the cases' runs say while they are made.
    pub(super) fn remarked(&mut self, remark: Remark) {
        self.remarked.push(remark);
    }

    /// The tally, once every case ran.
    pub(super) fn answered(&mut self, tested: Tested) {
        if let Progress::Running = self.progress {
            self.progress = Progress::Ran(tested);
        }
    }
}
