//! See [`super`]. Upkeep, under the machine, opened from it with `u`: what collecting what
//! runs left would reclaim, as `yunta gc --dry-run` prints it, and, with `c` once a person
//! confirms it removes what it lists, the collection, as `yunta gc` prints it, under a
//! headline that names upkeep. What the collection remarks stands as it comes, above the
//! page, as `yunta gc` says it before its page. Escape stops it, and what collecting would
//! reclaim is found again, since part of what the dry run listed may be gone. A page taller
//! than what shows of it scrolls. It stands in the frame of the field it was opened over
//! (`over_the_field`), where `a` goes where its attention row names. `n` opens a new run over
//! it, Escape goes back to the machine and `q` goes down to the field.

use chrono::{DateTime, Utc};
use surco::components::{Laid, Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::Remark;
use yunta_core::step::{Arg, NextStep};
use yunta_render::page::Collected;
use yunta_render::screen;

use super::over_the_field::{attending_over_the_field, in_the_fields_frame, KeyA};
use super::{Act, Want};
use crate::dialog::Confirmation;
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{FollowedRuns, Operation, Outcome};

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

    /// What the pass remarked, then what the last one reclaimed or would, as the screen
    /// tells `yunta gc`'s page, or that a pass runs; what refused, failed or was stopped; in
    /// the frame of the field `followed` keeps at `now`, a line typed in what stands over it
    /// where `typed`; with the keys.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        let mut doc = self.remarked.iter().fold(Doc::new(), |doc, remark| {
            doc.with(Note(crate::remark::said(remark, look.glyphs).words))
        });
        if let Some(collected) = &self.collected {
            doc = doc.with_all(collected.on_screen().into_blocks());
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
        let mut shown = Screen::new(Region::of_a_page(doc).in_sight(self.scroll.sight()));
        shown.trail = [screen::YUNTA, screen::MACHINE, screen::UPKEEP]
            .map(String::from)
            .to_vec();
        shown.legend = screen::upkeep_legend(self.would(), self.collecting());
        in_the_fields_frame(&mut shown, followed, now, typed, KeyA::Attends);
        shown
    }

    /// The work its screen says is under way: what collecting would reclaim found, or what
    /// runs left collected.
    pub(crate) fn shown_under_way(&self) -> Option<Operation> {
        (self.running).map(|pass| Operation::Collect {
            dry_run: pass == Pass::DryRun,
        })
    }

    /// Whether what stands is a dry run's, which `c` collects.
    fn would(&self) -> bool {
        self.running.is_none() && self.collected.as_ref().is_some_and(|last| last.dry_run)
    }

    /// Whether what runs left is being collected.
    fn collecting(&self) -> bool {
        self.running == Some(Pass::Collecting)
    }

    /// What `stroke` asks for here, over what `followed` keeps at `now`. While it collects,
    /// Escape stops it and finds what collecting would reclaim again. Otherwise `c` asks to
    /// confirm collecting what a dry run found, and Escape goes back. `a` goes where the
    /// attention row names, `n` opens a new run over the page, `q` goes down to the field,
    /// Ctrl-C leaves, and the keys that scroll scroll the page.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
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
            Stroke::Insert('q') => vec![Want::ToTheField(None)],
            Stroke::Interrupt => vec![Want::Leave],
            Stroke::Insert('c') if self.would() => {
                let collect = Operation::Collect { dry_run: false };
                let asks = screen::Confirming::Collect;
                let same = NextStep::Gc { dry_run: false };
                vec![Want::Confirm(Confirmation::new(asks, vec![same], collect))]
            }
            Stroke::Insert('a') => attending_over_the_field(followed, now),
            Stroke::Insert('n') if followed.can_ask_for_a_run() => Want::taking(&NextStep::Run {
                workflow: Arg::Blank,
            }),
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

    /// Where its page stands, which no row of its screen says, its legend the same whether
    /// it scrolls or not.
    pub(crate) fn page(&self) -> Scroll {
        self.scroll
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
            | Outcome::SetupPack { .. }
            | Outcome::Initialized { .. }
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::PullRequest(_)
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
