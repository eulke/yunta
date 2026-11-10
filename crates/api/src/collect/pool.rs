//! See [`super`]. What a pass of collection does to the project's pool: it
//! takes in the units' branches runs made before refs kept their work, and
//! it tidies the checkouts' builds within their budget, saying what the pool
//! keeps.

use yunta_render::page::{Reclaim, Uncollected};

use super::Collection;

impl Collection<'_> {
    /// Takes in the units' branches runs made before refs kept their work:
    /// what each unit committed stays, kept by its ref, and no list of the
    /// project's branches shows it.
    pub(super) async fn adopt(&mut self) {
        let adopted =
            yunta_engine::adopt_unit_branches(&self.pool, self.here(), None, self.supervision());
        match adopted.await {
            Ok(taken) if !taken.is_empty() => {
                let branches = taken.len();
                self.collected.did(Reclaim::UnitWorkKept { branches });
            }
            Ok(_) => {}
            Err(e) => self.problem(Uncollected::UnitBranches {
                failed: e.to_string(),
            }),
        }
    }

    /// Says which of the units' branches runs made before refs kept their
    /// work a pass would take in, taking none.
    pub(super) async fn would_adopt(&mut self) {
        let adoptable =
            yunta_engine::adoptable_unit_branches(&self.pool, self.here(), self.supervision());
        match adoptable.await {
            Ok(branches) if !branches.is_empty() => {
                let branches = branches.len();
                self.collected.did(Reclaim::UnitWorkKept { branches });
            }
            Ok(_) => {}
            Err(e) => self.problem(Uncollected::UnitBranches {
                failed: e.to_string(),
            }),
        }
    }

    /// Tidies the project's pool: what its checkouts' builds take is
    /// measured where nobody did, a build unused within retention is
    /// emptied, the builds stay within the budget, and a free checkout
    /// holding no build goes; then says what the pool keeps.
    pub(super) async fn tidy(&mut self) {
        let unused = self
            .project
            .config
            .storage
            .as_ref()
            .and_then(|storage| storage.retention_days)
            .map(|days| chrono::Duration::days(i64::from(days)));
        let tidied = match self.pool.tidy(unused, self.supervision()).await {
            Ok(tidied) => tidied,
            Err(e) => {
                return self.problem(Uncollected::Checkouts {
                    failed: e.to_string(),
                })
            }
        };
        for (checkout, bytes) in &tidied.emptied {
            self.collected.did(Reclaim::BuildEmptied {
                checkout: self.shown(checkout),
                bytes: *bytes,
            });
        }
        for checkout in &tidied.removed {
            self.collected.did(Reclaim::UnusedCheckoutRemoved {
                checkout: self.shown(checkout),
            });
        }
        self.collected.now = Some(tidied.kept.to_string());
    }
}
