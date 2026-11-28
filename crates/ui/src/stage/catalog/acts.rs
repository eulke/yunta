//! See [`super`]. What the catalog offers, each with the key that takes it there: on
//! the workflows, running, checking and drawing the one pointed at and reading its past
//! runs, a new workflow, the cases, the packs and the shapes of the documents Yunta
//! reads; on the packs, what they offer; on a page standing in their place, nothing. And the
//! keys help lists over what stands, those it takes beyond its legend among them.

use surco::components::Legend;
use yunta_core::step::{Arg, NextStep};
use yunta_render::screen::StageKeys;

use super::{CatalogStage, Shown};
use crate::prompt::Stroke;
use crate::stage::Act;
use crate::FollowedRuns;

impl CatalogStage {
    /// What the catalog offers, each with the key that takes it here: on the workflows,
    /// running the one pointed at where `followed` says a run can be asked for here, checking
    /// and drawing it and reading its past runs, a new workflow, the cases, the packs and the
    /// shapes; on the packs, what they offer; on a page standing in their place, nothing.
    pub(crate) fn offered(&self, followed: &FollowedRuns) -> Vec<Act> {
        match &self.shown {
            Shown::Catalog => self.offered_on_the_catalog(followed.can_ask_for_a_run()),
            Shown::Packs(packs) => packs.offered(),
            Shown::Checking { .. } | Shown::Checked { .. } | Shown::New(_) | Shown::Cases(_) => {
                Vec::new()
            }
        }
    }

    /// What the catalog offers on its workflows, a run of the one pointed at where one
    /// `asks_for_a_run` here.
    fn offered_on_the_catalog(&self, asks_for_a_run: bool) -> Vec<Act> {
        let pointed = self.pointed.iter().flat_map(|name| {
            let given = || Arg::Given(name.clone());
            let run = asks_for_a_run.then(|| (Stroke::Enter, NextStep::Run { workflow: given() }));
            run.into_iter().chain([
                (Stroke::Insert('c'), NextStep::Check { workflow: given() }),
                (Stroke::Insert('g'), NextStep::Graph { workflow: given() }),
            ])
        });
        let past = self
            .pointed_runs_kept_as()
            .map(|workflow| (Stroke::Insert('h'), NextStep::History { workflow }));
        let always = [
            (
                Stroke::Insert('n'),
                NextStep::NewWorkflow {
                    name: Arg::Blank,
                    shape: None,
                },
            ),
            (Stroke::Insert('t'), NextStep::Test { dir: None }),
            (Stroke::Insert('p'), NextStep::PackList),
            (Stroke::Insert('s'), NextStep::Schemas),
        ];
        pointed
            .chain(past)
            .chain(always)
            .map(|(key, step)| Act::by(key, step))
            .collect()
    }

    /// The keys help lists over what stands in the catalog's place, `page` saying those every
    /// page answers to: on the workflows, the acts beside the one pointed at, a new run of it
    /// by Enter where one `asks_for_a_run` here, `n` being a new workflow and nothing
    /// scrolling; on the packs' listing, `a` an audit and `n` a new pack, nothing scrolling; on
    /// a page standing in their place, Enter going back where it does as Escape does, and the
    /// keys that scroll where a page stands.
    pub(crate) fn keys<'l>(&self, page: StageKeys<'l>, asks_for_a_run: bool) -> StageKeys<'l> {
        match &self.shown {
            Shown::Catalog => {
                let reads = self.pointed_runs_kept_as().is_some();
                let acts = yunta_render::screen::workflow_acts(reads, asks_for_a_run);
                StageKeys {
                    beyond: self.pointed.as_ref().map(|_| acts).unwrap_or_default(),
                    new_run: false,
                    scrolls: false,
                    ..page
                }
            }
            Shown::Packs(packs) if packs.on_the_listing() => StageKeys {
                new_run: false,
                attends: false,
                scrolls: false,
                ..page
            },
            _ => StageKeys {
                beyond: match self.enter_goes_back() {
                    true => yunta_render::screen::enter_goes_back(),
                    false => Legend::new(),
                },
                scrolls: self.shows_a_page(),
                ..page
            },
        }
    }

    /// Whether Enter goes back to the workflows from what stands, as Escape does: a workflow's
    /// check, the cases once they ended, a workflow written, and a page the packs show once
    /// their work ended.
    fn enter_goes_back(&self) -> bool {
        match &self.shown {
            Shown::Checked { .. } => true,
            Shown::Cases(cases) => cases.ended(),
            Shown::New(asked) => asked.written().is_some(),
            Shown::Packs(packs) => packs.enter_goes_back(),
            Shown::Catalog | Shown::Checking { .. } => false,
        }
    }
}
