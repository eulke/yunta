//! See [`super`]. What the catalog offers, each with the key that takes it there: on
//! the workflows, running, checking and drawing the one pointed at and reading its past
//! runs, a new workflow, the cases, the packs and the shapes of the documents Yunta
//! reads; on the packs, what they offer; on a page standing in their place, nothing.

use yunta_core::step::{Arg, NextStep};

use super::{CatalogStage, Shown};
use crate::prompt::Stroke;
use crate::stage::Act;

impl CatalogStage {
    /// What the catalog offers, each with the key that takes it here: on the workflows,
    /// running, checking and drawing the one pointed at and reading its past runs, a new
    /// workflow, the cases, the packs and the shapes; on the packs, what they offer; on
    /// a page standing in their place, nothing.
    pub(crate) fn offered(&self) -> Vec<Act> {
        match &self.shown {
            Shown::Catalog => self.offered_on_the_catalog(),
            Shown::Packs(packs) => packs.offered(),
            Shown::Checking { .. } | Shown::Checked { .. } | Shown::New(_) | Shown::Cases(_) => {
                Vec::new()
            }
        }
    }

    /// What the catalog offers on its workflows.
    fn offered_on_the_catalog(&self) -> Vec<Act> {
        let pointed = self.pointed.iter().flat_map(|name| {
            let given = || Arg::Given(name.clone());
            [
                (Stroke::Enter, NextStep::Run { workflow: given() }),
                (Stroke::Insert('c'), NextStep::Check { workflow: given() }),
                (Stroke::Insert('g'), NextStep::Graph { workflow: given() }),
            ]
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
}
