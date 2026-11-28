//! See [`super`]. What work on the packs came to, on the page that waited on it: the listing
//! read, an audit, a pack removed, said and the listing read again, a new pack's page, a pack
//! fetched, installed or updated; what the work under way remarks; and what refused or failed,
//! said over the listing.

use std::sync::Arc;

use yunta_api::{AuditedPack, Remark};
use yunta_core::PackRef;
use yunta_render::page::pack::Listed;

use super::{Next, PacksShown, PacksView};
use crate::{Operation, PackOperation, PackOutcome, PackScaffolded};

impl PacksView {
    /// What work on the packs came to: the listing read, an audit, a pack removed
    /// (said, and the listing read again), or a new pack's page.
    pub(in crate::stage::catalog) fn answered(&mut self, outcome: PackOutcome) -> Next {
        match outcome {
            PackOutcome::Listed(listed) => self.listed(listed),
            PackOutcome::Audited { pack, audited } => self.audited(pack, audited),
            PackOutcome::Removed { pack } => return self.removed(pack),
            PackOutcome::Scaffolded(scaffolded) => self.scaffolded(scaffolded),
            PackOutcome::Fetched(fetched) => return self.fetched(fetched),
            PackOutcome::Installed(installed) => {
                if self.awaits_the_install() {
                    self.shown = PacksShown::Installed(installed);
                }
            }
            PackOutcome::Updated { pack, to, commit } => {
                if self.awaits_the_install() {
                    self.shown = PacksShown::Updated { pack, to, commit };
                }
            }
        }
        Next::Stays
    }

    /// The packs read: the pointer stays on the one it stood on while it is
    /// installed, and goes to the first otherwise.
    fn listed(&mut self, listed: Arc<Listed>) {
        let packs = listed.packs();
        let kept = self
            .pointed
            .as_ref()
            .is_some_and(|pointed| packs.contains(&pointed));
        if !kept {
            self.pointed = packs.first().map(|pack| (*pack).clone());
        }
        self.listed = Some(listed);
        self.reading = false;
    }

    /// `pack`'s audit, where it was being audited.
    fn audited(&mut self, pack: PackRef, audited: Arc<AuditedPack>) {
        if matches!(&self.shown, PacksShown::Auditing { pack: auditing } if *auditing == pack) {
            self.shown = PacksShown::Audited { pack, audited };
        }
    }

    /// `pack` removed: said over the listing, which is read again.
    fn removed(&mut self, pack: PackRef) -> Next {
        self.removed = Some(pack);
        self.shown = PacksShown::Listing;
        self.reading = true;
        Next::Work(Operation::Pack(PackOperation::List))
    }

    /// What writing a new pack came to, where one was being written.
    fn scaffolded(&mut self, scaffolded: Arc<PackScaffolded>) {
        if let PacksShown::Scaffolding { .. } = self.shown {
            self.shown = PacksShown::Scaffolded(scaffolded);
        }
    }

    /// What the work under way says while it works.
    pub(in crate::stage::catalog) fn remarked(&mut self, remark: Remark) {
        self.remarked.push(remark);
    }

    /// `said`, what `work` refused or failed on, said over the listing; where it was the
    /// listing's own reading, the packs are no longer said to be read.
    pub(in crate::stage::catalog) fn unanswered(&mut self, work: &PackOperation, said: String) {
        if matches!(work, PackOperation::List) {
            self.reading = false;
        }
        self.said = Some(said);
        self.shown = PacksShown::Listing;
    }
}
