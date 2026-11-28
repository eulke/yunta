//! See [`super`]. A new workflow, asked for in the catalog as `yunta new -i` asks it:
//! its name typed, then the shape its skeleton takes picked off the list `new -i`
//! offers, Escape taking the first as it does; then written and checked, the page
//! `yunta new` prints standing until Escape or Enter goes back to the workflows,
//! read again with the new one pointed at.

use surco::components::{ChoiceList, Legend, Note, Prose};
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::WorkflowShape;
use yunta_render::page::{NewWorkflow, Page};
use yunta_render::prompt::{shape_does, Escape, CHOOSE_A_SHAPE};

use super::Next;
use crate::prompt::{Alternative, Answerable, LineAsked, Picker, Press, Stroke};
use crate::{NewWorkflowWritten, Operation, WorkflowOperation};

/// A new workflow, from its name to what writing it came to.
#[derive(Debug, Clone)]
pub(super) enum NewWorkflowAsked {
    /// Its name, being typed.
    Naming(LineAsked),
    /// The shape the workflow `name` starts as, being picked.
    Shaping {
        name: String,
        picker: Picker<WorkflowShape>,
    },
    /// The workflow `name` being written from the skeleton of `shape`, and checked.
    Writing { name: String, shape: WorkflowShape },
    /// What writing it came to.
    Written(Box<NewWorkflowWritten>),
}

impl NewWorkflowAsked {
    /// A new workflow, its name asked first.
    pub(super) fn new() -> Self {
        let asks = yunta_render::screen::new_workflow_asks();
        NewWorkflowAsked::Naming(LineAsked::new(asks))
    }

    /// The question in front, the workflow being written, or its page; and the keys the
    /// screen lists, the shapes' in the legend's row as the documents' shapes are, a new run
    /// among them where one `asks_for_a_run` here.
    pub(super) fn screen(&self, look: &Look, asks_for_a_run: bool) -> (Doc, Legend) {
        match self {
            NewWorkflowAsked::Naming(line) => (
                Doc::new().with(line.block(look)),
                yunta_render::screen::naming_legend(),
            ),
            NewWorkflowAsked::Shaping { picker, .. } => {
                let asks = Prose(CHOOSE_A_SHAPE.to_string());
                let shapes = ChoiceList {
                    choices: picker.list(),
                    filter: picker.filter().map(str::to_string),
                    legend: Legend::new(),
                };
                let legend = yunta_render::screen::shapes_legend();
                (Doc::new().with(asks).with(shapes), legend)
            }
            NewWorkflowAsked::Writing { name, .. } => {
                let writing = Note(yunta_render::screen::writing_workflow(name));
                (Doc::new().with(writing), Legend::new())
            }
            NewWorkflowAsked::Written(written) => (
                written_page(written, look),
                yunta_render::screen::page_legend(
                    yunta_render::screen::PageRead::Whole,
                    Legend::new(),
                    asks_for_a_run,
                ),
            ),
        }
    }

    /// What `stroke` does here: the name answered, then the shape, which asks for
    /// the workflow written; Escape on the name goes back to the workflows, and on
    /// the shapes takes the first; on the page, Escape or Enter goes back.
    pub(super) fn pressed(&mut self, stroke: Stroke) -> Next {
        match self {
            NewWorkflowAsked::Naming(line) => match line.press(stroke) {
                Press::Answer(typed) => {
                    *self = NewWorkflowAsked::Shaping {
                        name: typed.value,
                        picker: shapes(),
                    };
                    Next::Stays
                }
                Press::Decline => Next::Done,
                Press::Interrupt => Next::Leave,
                Press::Stay | Press::Moved => Next::Stays,
            },
            NewWorkflowAsked::Shaping { name, picker } => {
                let shape = match picker.press(stroke) {
                    Press::Answer(shape) => shape,
                    Press::Decline => match picker.first() {
                        Some(first) => first,
                        None => return Next::Stays,
                    },
                    Press::Interrupt => return Next::Leave,
                    Press::Stay | Press::Moved => return Next::Stays,
                };
                let name = name.clone();
                *self = NewWorkflowAsked::Writing { name, shape };
                match self.shown_under_way() {
                    Some(writing) => Next::Work(writing),
                    None => Next::Stays,
                }
            }
            NewWorkflowAsked::Writing { .. } => match stroke {
                Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            NewWorkflowAsked::Written(_) => match stroke {
                Stroke::Decline | Stroke::Enter => Next::Done,
                Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
        }
    }

    /// The work its screen says is under way: the workflow written and checked, until it is.
    pub(super) fn shown_under_way(&self) -> Option<Operation> {
        match self {
            NewWorkflowAsked::Writing { name, shape } => {
                let new = WorkflowOperation::New {
                    name: name.clone(),
                    shape: *shape,
                };
                Some(Operation::Workflow(new))
            }
            NewWorkflowAsked::Naming(_)
            | NewWorkflowAsked::Shaping { .. }
            | NewWorkflowAsked::Written(_) => None,
        }
    }

    /// Whether every printable key types here: its name being typed, or the shapes being
    /// narrowed by what is typed.
    pub(super) fn typing(&self) -> bool {
        match self {
            NewWorkflowAsked::Naming(line) => line.typing(),
            NewWorkflowAsked::Shaping { .. } => true,
            NewWorkflowAsked::Writing { .. } | NewWorkflowAsked::Written(_) => false,
        }
    }

    /// What writing the workflow came to, where it was being written.
    pub(super) fn answered(&mut self, written: Box<NewWorkflowWritten>) {
        if let NewWorkflowAsked::Writing { .. } = self {
            *self = NewWorkflowAsked::Written(written);
        }
    }

    /// The name of the workflow written, once it is.
    pub(super) fn written(&self) -> Option<&str> {
        match self {
            NewWorkflowAsked::Written(written) => Some(&written.name),
            NewWorkflowAsked::Naming(_)
            | NewWorkflowAsked::Shaping { .. }
            | NewWorkflowAsked::Writing { .. } => None,
        }
    }
}

/// Every shape a new workflow starts as, each with what it does, on the list
/// `yunta new -i` puts them on: Escape takes the first, the plainest.
fn shapes() -> Picker<WorkflowShape> {
    let offered = WorkflowShape::all()
        .into_iter()
        .map(|shape| Alternative {
            label: Some(shape_does(shape).to_string()),
            ..Alternative::named(shape.label(), shape)
        })
        .collect();
    Picker::new(offered, Escape::TakesTheFirst)
}

/// The page `yunta new` prints of `written`, and what stopped its check where
/// something did.
fn written_page(written: &NewWorkflowWritten, look: &Look) -> Doc {
    let page = NewWorkflow {
        name: &written.name,
        shape: written.shape.label(),
        path: &written.path,
        at: &written.at,
        diagnosis: written.diagnosis.as_ref().ok(),
    };
    let doc = page.document(look);
    match &written.diagnosis {
        Ok(_) => doc,
        Err(unjudged) => doc.with(Note(unjudged.clone())),
    }
}
