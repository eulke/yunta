//! See [`super`]. The first request, in a repository Yunta was set up in that has no run
//! yet: what the person wants built, asked over the catalog of the workflows the repository
//! can run, as render words it ([`FirstRequest`]).
//!
//! Its screen: the name in block letters, the question over the box the words are typed in,
//! the box showing what a request reads like while it is empty, and each workflow with what
//! it does, all set to a line's measure ([`MEASURE`]) and centred on a wider screen; under
//! them the field in perspective, in every row they leave. The trail names the repository's
//! path as the repository's reading gave it. The tally and the attention row are the frame
//! `FollowedRuns::framing` composes for a screen that shows no run and where a line is typed,
//! so the row names what needs the person with no key. The legend says what typing does,
//! then `enter` and `esc`, with no far pair, since every printable key types here.
//!
//! What stands and what moves it: the question and what is typed stand while the person is on
//! them, whatever the field followed beneath says, so a run started elsewhere shows in the
//! tally and the attention row and never takes them away; the stage stands down only by the
//! person's key. Printable keys type. `enter` goes on with what is typed to a new run, the
//! request in its row, which reads what it needs as it opens; going back from that new run
//! puts the request, as it was edited there, back in this line. `esc` goes to the
//! repository's field whatever it holds, as its legend says, and the field takes the first
//! request's place for good: with no run in it, it lists its first row alone, and its `n`
//! opens a new run that asks the same question over the catalog. Ctrl-C leaves. A person who
//! comes back to the first request once the field holds a run finds the field in its place,
//! as the state keeps it.

use chrono::{DateTime, Utc};
use surco::components::{Horizon, Measure, Note, Region, Screen, Wordmark};
use surco::doc::Doc;
use surco::line::Tone;
use surco::look::Look;
use yunta_api::{FieldReach, Repository};
use yunta_render::page::{Catalog, FirstRequest, Page};

use super::{FieldStage, Stage, StartStage, Want};
use crate::followed_runs::Standpoint;
use crate::prompt::{Edit, Stroke, TypedLine};
use crate::{FollowedRuns, Operation, Outcome};

/// The cells the first request's words stand in, centred on a wider screen: a request of a
/// few words typed with room to spare, and the question, its box and the workflows read in
/// one sweep of the eye, as the design sets them.
const MEASURE: usize = 72;

/// The first run being asked for.
#[derive(Debug, Clone)]
pub struct FirstRunStage {
    /// What the person wants built, being typed.
    request: TypedLine,
    /// The workflows the repository can run, once read.
    catalog: Option<Box<Catalog>>,
    /// What the last piece of work came to where it did not do what was asked.
    said: Option<String>,
}

impl FirstRunStage {
    /// The question, with nothing typed and the catalog yet to be read.
    pub(crate) fn new() -> Self {
        FirstRunStage {
            request: TypedLine::default(),
            catalog: None,
            said: None,
        }
    }

    /// What the stage asks for as it opens: the catalog.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![Want::Work(Operation::ReadCatalog)]
    }

    /// What the stage asks for when a person comes back to it: the field in its place, where
    /// the field `followed` keeps holds a run started since, read or not.
    pub(crate) fn returned(&self, followed: &FollowedRuns) -> Vec<Want> {
        let holds_a_run = (followed.field.as_ref()).is_some_and(|field| field.holds_a_run());
        holds_a_run
            .then(the_field_in_its_place)
            .into_iter()
            .collect()
    }

    /// The first request as a screen over what `followed` keeps at `now`: the name, the
    /// question over its box and the workflows, set to the first request's measure over the
    /// field in perspective; where it stands on the trail; the tally and the attention row,
    /// a line typed here; and the keys.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Screen {
        let (before, after) = self.request.split();
        let asked = FirstRequest {
            before: &before,
            after: &after,
            catalog: self.catalog.as_deref(),
        };
        let said = self.said.iter().map(|said| Note(said.clone()).into());
        let doc = Doc::new()
            .with(Wordmark)
            .with_all(asked.document(look).into_blocks())
            .with_all(said)
            .with(Horizon { tone: Tone::Faint });
        let measure = Measure::of(MEASURE).lowered();
        let mut screen = Screen::new(Region::new(doc).measured(measure));
        let home = followed.home.as_deref();
        let place = followed
            .repository
            .as_ref()
            .and_then(Repository::dir)
            .map(|repository| yunta_render::screen::field_place(Some(repository), home));
        screen.trail = std::iter::once(yunta_render::screen::YUNTA.to_string())
            .chain(place)
            .collect();
        let at = Standpoint {
            run: None,
            typed: true,
        };
        (screen.tally, screen.attention) = followed.framing(at, now);
        screen.legend = yunta_render::screen::first_request_legend();
        screen.typed = true;
        screen
    }

    /// What `stroke` asks for here: the request typed, and once it is answered, taken to a
    /// new run as its request, whatever the first request read; `esc` going to the field in
    /// its place, whatever the field holds; leaving on an interrupt.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        self.said = None;
        match self.request.apply(stroke) {
            Edit::Answered(typed) => {
                let request = TypedLine::holding(&typed.line);
                vec![Want::Open(Stage::Start(StartStage::asked(request)))]
            }
            Edit::Declined => vec![the_field_in_its_place()],
            Edit::Interrupted => vec![Want::Leave],
            Edit::Editing => Vec::new(),
        }
    }

    /// The request as it was typed on the new run a person came back from, in its line.
    pub(crate) fn typed_again(&mut self, request: TypedLine) {
        self.request = request;
    }

    /// Whether the request is being typed: it always is, here.
    pub(crate) fn typing(&self) -> bool {
        true
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
            | Outcome::PullRequest(_)
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
            | Outcome::SetupPack { .. }
            | Outcome::Initialized { .. }
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::Stopped => Vec::new(),
        }
    }
}

/// The repository's field in the first request's place, at the bottom of what stands, which
/// the first request leaves for good: where `esc` goes, whatever the field holds, and what a
/// person coming back finds once it holds a run, as the application stands on the field
/// wherever it holds one.
fn the_field_in_its_place() -> Want {
    Want::Replace(Stage::Field(FieldStage::reaching(FieldReach::Here)))
}
