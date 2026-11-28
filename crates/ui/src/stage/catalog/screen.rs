//! See [`super`]. The catalog as a screen: the workflows a row each, the one pointed at in a
//! panel beside them at the right, which stands under them where the screen gives it no room,
//! or the page standing in their place (a workflow's check, the cases, a new workflow, the
//! packs), each standing as a page, a line typed or a list narrowed by typing on the screen
//! itself where one is asked, as wide as a new run's request at most; in the frame of the
//! field it was opened over.

use chrono::{DateTime, Utc};
use surco::components::{Legend, Measure, Note, Region, Screen, Side, SideAt, Sight};
use surco::doc::{Block, Doc};
use surco::look::Look;
use yunta_api::WorkflowCheck;
use yunta_render::page::{Page, PointedCatalog, WhatItTakes};

use super::{CatalogStage, Shown};
use crate::stage::over_the_field::{in_the_fields_frame, KeyA};
use crate::FollowedRuns;

impl CatalogStage {
    /// The catalog pointed at, the one pointed at beside it, or the page standing in its
    /// place; what refused or failed under it; in the frame of the field `followed` keeps at
    /// `now`, a line typed in what stands over it where `typed`; and the keys.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        let asks_for_a_run = followed.can_ask_for_a_run();
        let (place, mut doc, legend) = self.shown(look, asks_for_a_run);
        let trail = [yunta_render::screen::YUNTA, yunta_render::screen::WORKFLOWS]
            .map(String::from)
            .into_iter()
            .chain(place)
            .collect();
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let region = match &self.shown {
            Shown::Catalog => Region::new(doc),
            _ if self.typing() => Region::of_a_page(doc).measured(question()),
            _ => Region::of_a_page(doc),
        };
        let mut screen = Screen::new(region.in_sight(self.sight()));
        screen.trail = trail;
        screen.legend = legend;
        if let Shown::Catalog = self.shown {
            screen.side = self.panel(look, asks_for_a_run);
        }
        screen.typed = self.typing();
        let typed = typed || screen.typed;
        let a = match self.takes_itself('a') {
            true => KeyA::TheStagesOwn,
            false => KeyA::Attends,
        };
        in_the_fields_frame(&mut screen, followed, now, typed, a);
        screen
    }

    /// What stands, where it stands on the trail after the workflows, and its keys, a new run
    /// among them where one `asks_for_a_run` here: the catalog pointed at; a workflow being
    /// checked, or its check, by its name and the check's word; a new workflow, the cases or
    /// the packs, each by its own.
    fn shown(&self, look: &Look, asks_for_a_run: bool) -> (Vec<String>, Doc, Legend) {
        let checked_place =
            |name: &str| yunta_render::screen::place_of(name, yunta_render::screen::CHECK);
        match &self.shown {
            Shown::Catalog => {
                let legend = yunta_render::screen::catalog_legend(asks_for_a_run);
                (Vec::new(), self.listed(look), legend)
            }
            Shown::Checking { name } => {
                let checking = Note(yunta_render::screen::checking(name));
                (
                    vec![checked_place(name)],
                    Doc::new().with(checking),
                    working_legend(),
                )
            }
            Shown::Checked { name, check } => {
                let legend = yunta_render::screen::page_legend(
                    yunta_render::screen::PageRead::Whole,
                    Legend::new(),
                    asks_for_a_run,
                );
                (vec![checked_place(name)], checked(check, look), legend)
            }
            Shown::New(asked) => {
                let (doc, legend) = asked.screen(look, asks_for_a_run);
                let place = yunta_render::screen::NEW_WORKFLOW.to_string();
                (vec![place], doc, legend)
            }
            Shown::Cases(cases) => {
                let (doc, legend) = cases.screen(look, asks_for_a_run);
                (vec![yunta_render::screen::CASES.to_string()], doc, legend)
            }
            Shown::Packs(packs) => {
                let place = std::iter::once(yunta_render::screen::PACKS.to_string());
                let (doc, legend) = packs.screen(look, asks_for_a_run);
                (place.chain(packs.trail()).collect(), doc, legend)
            }
        }
    }

    /// The catalog as a person moves through it, or that it is being read: nothing where its
    /// reading ended with none read, what refused or failed it said under it.
    fn listed(&self, look: &Look) -> Doc {
        let Some(catalog) = &self.catalog else {
            let reading = yunta_render::screen::READING_THE_WORKFLOWS.to_string();
            let reading = self.reading.then(|| Note(reading).into());
            return Doc::new().with_all(reading);
        };
        let page = PointedCatalog {
            catalog,
            pointed: self.pointed.as_deref().unwrap_or_default(),
        };
        page.on_screen(look)
    }

    /// The panel beside the catalog, at the right, where a new run's stands: under the name of
    /// the workflow pointed at, what `yunta list` says of it and what can be done with it
    /// here, each with its key, a new run of it where one `asks_for_a_run` here; the same under
    /// the workflows, its name over it, where the screen gives the panel no room. None until
    /// the catalog is read, or where nothing is pointed at.
    fn panel(&self, look: &Look, asks_for_a_run: bool) -> Option<Side> {
        let (catalog, pointed) = (self.catalog.as_deref()?, self.pointed.as_deref()?);
        let reads = self.pointed_runs_kept_as().is_some();
        let page = PointedCatalog { catalog, pointed };
        let acts = || yunta_render::screen::workflow_acts(reads, asks_for_a_run);
        let panel = || page.panel(look, acts());
        let named = Block::Heading(pointed.to_string());
        let instead = std::iter::once(named)
            .chain(panel().into_blocks())
            .collect();
        Some(Side {
            region: Region::new(panel())
                .titled([pointed])
                .measured(Measure::of(WhatItTakes::CELLS))
                .in_sight(Sight::default()),
            at: SideAt::Right,
            focused: false,
            instead,
        })
    }
}

/// The keys of a page whose work runs and which Escape goes back from, letting the work go:
/// a workflow being checked, a pack being audited or fetched.
pub(super) fn working_legend() -> Legend {
    yunta_render::screen::page_legend(yunta_render::screen::PageRead::Whole, Legend::new(), false)
}

/// The measure a line asked on a page stands in: as wide as a new run's request beside its
/// panel at most, at the page's left edge, the page's width where that is narrower.
fn question() -> Measure {
    Measure::of(WhatItTakes::QUESTION_CELLS).from_the_left()
}

/// What checking a workflow came to, as `yunta check` prints it: the verdict's page,
/// then what its past runs say, where they say anything.
fn checked(check: &WorkflowCheck, look: &Look) -> Doc {
    let doc = check.checked.document(look);
    match check.findings.is_empty() {
        true => doc,
        false => doc.with_all(check.findings.document(look).into_blocks()),
    }
}
