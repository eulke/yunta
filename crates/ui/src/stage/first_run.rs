//! See [`super`]. The first run, in a repository Yunta was set up in that has no run
//! yet: what the person wants built, asked over the catalog of the workflows the
//! repository can run. What they type is taken to the start stage, which picks the
//! workflow for it. A person who comes back once the repository has a run finds its
//! field in place of the question.

use surco::components::{Note, Region, Screen, Sight};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::FieldReach;
use yunta_render::page::{Catalog, Page};

use super::{FieldStage, Stage, StartStage, Want};
use crate::prompt::{Answerable, LineAsked, Press, Stroke};
use crate::{Operation, Outcome};

/// The first run being asked for.
#[derive(Debug, Clone)]
pub struct FirstRunStage {
    /// What the person wants built, being typed.
    request: LineAsked,
    /// The workflows the repository can run, once read.
    catalog: Option<Box<Catalog>>,
    /// What the last piece of work came to where it did not do what was asked.
    said: Option<String>,
}

impl FirstRunStage {
    /// The question, with nothing typed and the catalog yet to be read.
    pub(crate) fn new() -> Self {
        FirstRunStage {
            request: LineAsked::new(yunta_render::screen::first_run_asks()),
            catalog: None,
            said: None,
        }
    }

    /// What the stage asks for as it opens: the catalog.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![Want::Work(Operation::ReadCatalog)]
    }

    /// What the stage asks for when a person comes back to it: the repository's runs,
    /// which a run started since has given it.
    pub(crate) fn returned(&self) -> Vec<Want> {
        vec![Want::Work(Operation::Field {
            reach: FieldReach::Here,
        })]
    }

    /// The question over the catalog, and the keys.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let mut doc = Doc::new().with(self.request.block(look));
        if let Some(catalog) = &self.catalog {
            doc = doc.with_all(catalog.document(look).into_blocks());
        }
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut screen = Screen::new(Region::new(doc).in_sight(Sight::following_pointer()));
        screen.trail = vec![yunta_render::screen::YUNTA.to_string()];
        screen.legend = yunta_render::screen::first_run_legend();
        screen
    }

    /// What `stroke` asks for here: the request typed, and once it is answered, taken
    /// to the start stage; leaving leaves the application.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        self.said = None;
        match self.request.press(stroke) {
            Press::Answer(typed) => match &self.catalog {
                Some(catalog) => vec![Want::Open(Stage::Start(StartStage::asked(
                    typed.value,
                    catalog.clone(),
                )))],
                None => Vec::new(),
            },
            Press::Decline | Press::Interrupt => vec![Want::Leave],
            Press::Stay | Press::Moved => Vec::new(),
        }
    }

    /// Whether the request is being typed: it always is, here.
    pub(crate) fn typing(&self) -> bool {
        self.request.typing()
    }

    /// What an answer to work the stage asked for does: the catalog read, or the
    /// repository's runs, which once there are any give the field the question's
    /// place; what refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Catalog(catalog) => {
                self.catalog = Some(catalog);
                Vec::new()
            }
            Outcome::Field { reading, storage }
                if !reading.rows.is_empty() || !reading.unreadable.is_empty() =>
            {
                let field = FieldStage::read(reading, storage, FieldReach::Here);
                vec![Want::Replace(Stage::Field(field))]
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
            | Outcome::Field { .. }
            | Outcome::Started { .. }
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
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Detected { .. }
            | Outcome::Initialized(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::Stopped => Vec::new(),
        }
    }
}
