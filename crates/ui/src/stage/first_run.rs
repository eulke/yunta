//! See [`super`]. The first run, in a repository Yunta was set up in that has no run
//! yet: what the person wants built, asked over the catalog of the workflows the
//! repository can run. What they type is taken to the start stage, which picks the
//! workflow for it.
//!
//! The question stands while the person is on it, whatever the field followed beneath
//! it says: a run that comes into the field never takes what they are typing away. A
//! person who comes back to it once the field holds a run finds the field in its place,
//! as the state keeps it.

use surco::components::{Note, Region, Screen, Sight};
use surco::doc::Doc;
use surco::look::Look;
use yunta_render::page::{Catalog, Page};

use super::{FieldStage, Stage, StartStage, Want};
use crate::prompt::{Answerable, LineAsked, Press, Stroke};
use crate::{FollowedRuns, Operation, Outcome};

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

    /// What the stage asks for when a person comes back to it: the field `followed` keeps
    /// in its place, where a run started since stands in it, read or not.
    pub(crate) fn returned(&self, followed: &FollowedRuns) -> Vec<Want> {
        let field = followed.field.as_ref().filter(|field| field.holds_a_run());
        field
            .map(|field| Want::Replace(Stage::Field(FieldStage::reaching(field.reach))))
            .into_iter()
            .collect()
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

    /// What an answer to work the stage asked for does: the catalog read; what refused
    /// or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Catalog(catalog) => {
                self.catalog = Some(catalog);
                Vec::new()
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
            | Outcome::Started { .. }
            | Outcome::Page(_)
            | Outcome::NodePage(_)
            | Outcome::Questions(_)
            | Outcome::Decision(_)
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
