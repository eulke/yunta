//! See [`super`]. Upkeep, opened from doctor with `u`: what collecting what runs left
//! would reclaim, as `yunta gc --dry-run` prints it, and, with `c` once a person
//! confirms it removes what it lists, the collection, as `yunta gc` prints it. What the
//! collection remarks stands as it comes, above the page, as `yunta gc` says it before
//! its page. Escape stops it, and what collecting would reclaim is found again, since
//! part of what the dry run listed may be gone. A page taller than what
//! shows of it scrolls. Escape goes back to doctor.

use surco::components::{Laid, Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::Remark;
use yunta_core::step::NextStep;
use yunta_render::page::{Collected, Page};
use yunta_render::screen;

use super::{Act, Want};
use crate::dialog::Confirmation;
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{Operation, Outcome};

/// What collecting what runs left would reclaim, then what it reclaimed.
#[derive(Debug, Clone)]
pub struct UpkeepStage {
    /// What the last pass reclaimed, or would, once it ended.
    collected: Option<Box<Collected>>,
    /// The pass under way, where one is.
    running: Option<Pass>,
    /// What the pass under way, or the last one, remarked.
    remarked: Vec<Remark>,
    /// Where the page is scrolled to.
    scroll: Scroll,
    /// What refused, failed or was stopped, where something was.
    said: Option<String>,
}

/// Which pass is under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// What collecting would reclaim, found with nothing removed.
    DryRun,
    /// What runs left, collected.
    Collecting,
}

impl UpkeepStage {
    /// Upkeep, its dry run yet to run.
    pub(crate) fn new() -> Self {
        UpkeepStage {
            collected: None,
            running: Some(Pass::DryRun),
            remarked: Vec::new(),
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What the stage asks for as it opens: what collecting would reclaim.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![Want::Work(Operation::Collect { dry_run: true })]
    }

    /// What the pass remarked, then what the last one reclaimed or would, as `yunta gc`
    /// prints it, or that a pass runs; what refused, failed or was stopped; with the
    /// keys.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let mut doc = self.remarked.iter().fold(Doc::new(), |doc, remark| {
            doc.with(Note(crate::remark::said(remark, look.glyphs).words))
        });
        if let Some(collected) = &self.collected {
            doc = doc.with_all(collected.document(look).into_blocks());
        }
        let running = match self.running {
            Some(Pass::DryRun) => Some(screen::FINDING_WHAT_IT_RECLAIMS),
            Some(Pass::Collecting) => Some(screen::COLLECTING),
            None => None,
        };
        if let Some(running) = running {
            doc = doc.with(Note(running.to_string()));
        }
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut shown = Screen::new(Region::new(doc).in_sight(self.scroll.sight()));
        shown.trail = vec![screen::YUNTA.to_string(), screen::UPKEEP.to_string()];
        let legend = screen::upkeep_legend(self.would(), self.collecting());
        shown.legend = match self.scroll.scrolls() {
            true => screen::scrolling(legend),
            false => legend,
        };
        shown
    }

    /// Whether what stands is a dry run's, which `c` collects.
    fn would(&self) -> bool {
        self.running.is_none() && self.collected.as_ref().is_some_and(|last| last.dry_run)
    }

    /// Whether what runs left is being collected.
    fn collecting(&self) -> bool {
        self.running == Some(Pass::Collecting)
    }

    /// What `stroke` asks for here. While it collects, Escape stops it and finds what
    /// collecting would reclaim again. Otherwise `c`
    /// asks to confirm collecting what a dry run found, and Escape goes back. `q` or
    /// Ctrl-C leaves, and the keys that scroll scroll the page.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        match stroke {
            Stroke::Decline if self.collecting() => {
                self.running = Some(Pass::DryRun);
                self.said = Some(screen::COLLECTION_STOPPED.to_string());
                vec![
                    Want::LetGo(Operation::Collect { dry_run: false }),
                    Want::Work(Operation::Collect { dry_run: true }),
                ]
            }
            Stroke::Decline => vec![Want::Back],
            Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
            Stroke::Insert('c') if self.would() => {
                let collect = Operation::Collect { dry_run: false };
                let asks = screen::COLLECTING_ASKS.to_string();
                let same = NextStep::Gc { dry_run: false };
                vec![Want::Confirm(Confirmation::new(asks, vec![same], collect))]
            }
            other => {
                self.scroll.pressed(other);
                Vec::new()
            }
        }
    }

    /// What upkeep offers, with the key that takes it here: collecting what a dry run
    /// found.
    pub(crate) fn offered(&self) -> Vec<Act> {
        match self.would() {
            true => vec![Act::by(
                Stroke::Insert('c'),
                NextStep::Gc { dry_run: false },
            )],
            false => Vec::new(),
        }
    }

    /// What an act the stage asked to be confirmed does once it is: the collection
    /// runs, and what it remarks stands as it comes.
    pub(crate) fn confirmed(&mut self, operation: Operation) -> Vec<Want> {
        if operation == (Operation::Collect { dry_run: false }) {
            self.running = Some(Pass::Collecting);
            self.remarked.clear();
            self.said = None;
        }
        vec![Want::Work(operation)]
    }

    /// What the pass under way remarks, which stands as it comes.
    pub(crate) fn remarked(&mut self, remark: Remark) {
        self.remarked.push(remark);
    }

    /// What a surface laid out of the page, which the keys that scroll move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        self.scroll.laid(laid);
    }

    /// What an answer to a pass does: what it reclaimed, or would, stands, from its
    /// first line; what refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Collected(collected) => {
                self.collected = Some(collected);
                self.running = None;
                self.scroll.rewound();
            }
            Outcome::Refused(refused) => {
                self.running = None;
                self.said = Some(refused.to_string());
            }
            Outcome::Failed(said) => {
                self.running = None;
                self.said = Some(said);
            }
            Outcome::Repository(_)
            | Outcome::Detected { .. }
            | Outcome::Initialized(_)
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
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
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Started { .. }
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Stopped => {}
        }
        Vec::new()
    }
}
