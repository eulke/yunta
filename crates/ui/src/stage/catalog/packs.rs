//! See [`super`]. The packs installed, as `yunta pack list` lists them, with one
//! pointed at and moved through with `j` and `k`. `a` audits the one pointed at, its
//! own cases run on it, as `yunta pack audit` does; `x` removes it once a person
//! confirmed, and the packs are read again; `n` asks for a new pack's publisher and
//! name and writes it as `yunta pack new` does. `i` asks where a pack is and `u` the
//! ref the one pointed at moves to: the pack is fetched, its copy held under the
//! fetch's ticket, and its inventory read; Enter installs that copy, never fetched
//! again, a confirmation naming what it executes first where that waits on a
//! person, and Escape lets the copy go. An update that ships no executor installs
//! as soon as it is fetched, as `yunta pack update` does. Each page stands in the
//! listing's place until Escape takes the person back to it.

mod install;

use std::mem::{discriminant, Discriminant};
use std::sync::Arc;

use surco::components::{Legend, Note};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::{AuditedPack, Remark};
use yunta_core::step::{Arg, NextStep};
use yunta_core::{CommitSha, PackRef};
use yunta_render::page::pack::{Inventory, Listed, NewPack, PointedPacks, Removed};
use yunta_render::page::Page;

use super::super::Act;
use super::Next;
use crate::dialog::Confirmation;
use crate::prompt::{Answerable, LineAsked, Press, Stroke};
use crate::{
    Operation, PackFetched, PackInstalled, PackOperation, PackOutcome, PackScaffolded, Ticket,
};
use install::{installing_screen, PackAsked};

/// The packs installed, one pointed at, and the page standing in their place.
#[derive(Debug, Clone)]
pub(super) struct PacksView {
    /// The packs, once read.
    listed: Option<Arc<Listed>>,
    /// The pack the pointer stands on.
    pointed: Option<PackRef>,
    shown: PacksShown,
    /// The pack removed last, said over the listing until the next stroke.
    removed: Option<PackRef>,
    /// What the work under way said while it worked: a pack's cases, run.
    remarked: Vec<Remark>,
    /// What refused or failed, where something did.
    said: Option<String>,
}

/// What stands where the packs are, by its kind alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PacksStanding(Discriminant<PacksShown>);

/// What stands where the packs are.
#[derive(Debug, Clone)]
enum PacksShown {
    /// The listing, pointed at.
    Listing,
    /// The pack being audited.
    Auditing { pack: PackRef },
    /// A pack's audit, and what its own cases did.
    Audited {
        pack: PackRef,
        audited: Arc<AuditedPack>,
    },
    /// A new pack's publisher and name, being typed.
    Naming(LineAsked),
    /// The new pack, as typed, being written and checked.
    Scaffolding { pack: String },
    /// What writing a new pack came to.
    Scaffolded(Arc<PackScaffolded>),
    /// Where a pack to install is, or the ref an installed one moves to, being
    /// typed.
    Asking(PackAsked),
    /// A pack being fetched by `work`, `what` it is as the person named it.
    Fetching { work: PackOperation, what: String },
    /// A pack fetched, its copy held, its inventory read.
    Fetched(Arc<PackFetched>),
    /// The pack being installed from its copy.
    Installing { pack: PackRef },
    /// What installing a pack new to the repository came to.
    Installed(Arc<PackInstalled>),
    /// What updating an installed pack came to.
    Updated {
        pack: PackRef,
        to: String,
        commit: CommitSha,
    },
}

impl PacksView {
    /// The packs, being read.
    pub(super) fn reading() -> Self {
        PacksView {
            listed: None,
            pointed: None,
            shown: PacksShown::Listing,
            removed: None,
            remarked: Vec::new(),
            said: None,
        }
    }

    /// The listing, an audit, a new pack's question or its page, what the work
    /// under way said, and what refused or failed; with the keys.
    pub(super) fn screen(&self, look: &Look) -> (Doc, Legend) {
        let (mut doc, legend) = match &self.shown {
            PacksShown::Listing => (self.listing(look), yunta_render::screen::packs_legend()),
            PacksShown::Auditing { pack } => {
                let auditing = Note(yunta_render::screen::auditing(&pack.to_string()));
                (Doc::new().with(auditing), Legend::new())
            }
            PacksShown::Audited { pack, audited } => {
                let inventory = Inventory::Audited {
                    audit: &audited.audit,
                    pack,
                    cases: &audited.cases,
                };
                let legend = yunta_render::screen::pack_page_legend();
                (inventory.document(look), legend)
            }
            PacksShown::Naming(line) => (
                Doc::new().with(line.block(look)),
                yunta_render::screen::pack_naming_legend(),
            ),
            PacksShown::Scaffolding { pack } => {
                let writing = Note(yunta_render::screen::scaffolding(pack));
                (Doc::new().with(writing), Legend::new())
            }
            PacksShown::Scaffolded(scaffolded) => (
                scaffolded_page(scaffolded, look),
                yunta_render::screen::pack_page_legend(),
            ),
            other => installing_screen(other, look),
        };
        for remark in &self.remarked {
            doc = doc.with(Note(crate::remark::said(remark, look.glyphs).words));
        }
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        (doc, legend)
    }

    /// The pack removed last, then the packs pointed at, or that they are read.
    fn listing(&self, look: &Look) -> Doc {
        let mut doc = Doc::new();
        if let Some(pack) = &self.removed {
            doc = doc.with_all(Removed { pack }.document(look).into_blocks());
        }
        let Some(listed) = &self.listed else {
            let reading = yunta_render::screen::READING_THE_PACKS.to_string();
            return doc.with(Note(reading));
        };
        let page = match &self.pointed {
            Some(pointed) => PointedPacks { listed, pointed }.document(look),
            None => listed.document(look),
        };
        doc.with_all(page.into_blocks())
    }

    /// Where the trail goes on past the packs: the pack an audit is of, or a new
    /// pack.
    pub(super) fn trail(&self) -> Option<String> {
        match &self.shown {
            PacksShown::Listing => None,
            PacksShown::Auditing { pack } | PacksShown::Audited { pack, .. } => {
                Some(pack.to_string())
            }
            PacksShown::Naming(_) | PacksShown::Scaffolding { .. } | PacksShown::Scaffolded(_) => {
                Some(yunta_render::screen::NEW.to_string())
            }
            PacksShown::Asking(asked) => Some(asked.trail()),
            PacksShown::Fetching { .. } => Some(yunta_render::screen::INSTALL.to_string()),
            PacksShown::Installing { pack } | PacksShown::Updated { pack, .. } => {
                Some(pack.to_string())
            }
            PacksShown::Fetched(fetched) => Some(fetched.pack.to_string()),
            PacksShown::Installed(installed) => Some(installed.manifest.reference().to_string()),
        }
    }

    /// The ticket the performer holds a fetched pack's copy under, while its
    /// install is asked.
    pub(super) fn held(&self) -> Option<Ticket> {
        match &self.shown {
            PacksShown::Fetched(fetched) => Some(fetched.held),
            _ => None,
        }
    }

    /// What `stroke` does here: the listing's keys, or what the page in its place
    /// makes of it.
    pub(super) fn pressed(&mut self, stroke: Stroke) -> Next {
        match &mut self.shown {
            PacksShown::Listing => self.pressed_on_the_listing(stroke),
            PacksShown::Auditing { pack } => match stroke {
                Stroke::Decline => {
                    let audit = PackOperation::Audit { pack: pack.clone() };
                    self.shown = PacksShown::Listing;
                    Next::LetGo(Operation::Pack(audit))
                }
                Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            PacksShown::Naming(line) => match line.press(stroke) {
                Press::Answer(typed) => {
                    let pack = typed.value;
                    self.shown = PacksShown::Scaffolding { pack: pack.clone() };
                    self.remarked.clear();
                    Next::Work(Operation::Pack(PackOperation::Scaffold { pack }))
                }
                Press::Decline => {
                    self.shown = PacksShown::Listing;
                    Next::Stays
                }
                Press::Interrupt => Next::Leave,
                Press::Stay | Press::Moved => Next::Stays,
            },
            PacksShown::Scaffolding { .. } => match stroke {
                Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            PacksShown::Audited { .. } | PacksShown::Scaffolded(_) => match stroke {
                Stroke::Decline | Stroke::Enter => {
                    self.shown = PacksShown::Listing;
                    self.remarked.clear();
                    Next::Stays
                }
                Stroke::Insert('q') | Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            _ => self.pressed_installing(stroke),
        }
    }

    /// Whether a page stands where the packs are, which scrolls: an audit, what a new pack
    /// came to, a fetched pack's inventory, what installing or updating one came to.
    pub(super) fn shows_a_page(&self) -> bool {
        matches!(
            self.shown,
            PacksShown::Audited { .. }
                | PacksShown::Scaffolded(_)
                | PacksShown::Fetched(_)
                | PacksShown::Installed(_)
                | PacksShown::Updated { .. }
        )
    }

    /// What stands where the packs are, by its kind: what a page standing for another
    /// starts from its first line again by.
    pub(super) fn standing(&self) -> PacksStanding {
        PacksStanding(discriminant(&self.shown))
    }

    /// What the packs offer on their listing, each with the key that takes it here:
    /// auditing, removing and updating the one pointed at, and installing one. A new
    /// pack, whose command needs the name it is given, is offered by its key alone.
    pub(super) fn offered(&self) -> Vec<Act> {
        if !matches!(self.shown, PacksShown::Listing) {
            return Vec::new();
        }
        let pointed = self.pointed.iter().flat_map(|pack| {
            [
                (
                    Stroke::Insert('a'),
                    NextStep::PackAudit { pack: pack.clone() },
                ),
                (
                    Stroke::Insert('x'),
                    NextStep::PackRemove { pack: pack.clone() },
                ),
                (
                    Stroke::Insert('u'),
                    NextStep::PackUpdate {
                        pack: pack.clone(),
                        reference: Arg::Blank,
                    },
                ),
            ]
        });
        let install = (
            Stroke::Insert('i'),
            NextStep::PackAdd { source: Arg::Blank },
        );
        pointed
            .chain([install])
            .map(|(key, step)| Act::by(key, step))
            .collect()
    }

    /// What `stroke` asks for on the listing.
    fn pressed_on_the_listing(&mut self, stroke: Stroke) -> Next {
        self.removed = None;
        self.said = None;
        match stroke {
            Stroke::Down | Stroke::Tab | Stroke::Insert('j') => self.moved(1),
            Stroke::Up | Stroke::BackTab | Stroke::Insert('k') => self.moved(-1),
            Stroke::Insert('a') => match self.pointed.clone() {
                Some(pack) => {
                    self.shown = PacksShown::Auditing { pack: pack.clone() };
                    self.remarked.clear();
                    Next::Work(Operation::Pack(PackOperation::Audit { pack }))
                }
                None => Next::Stays,
            },
            Stroke::Insert('x') => match self.pointed.clone() {
                Some(pack) => {
                    let asks = yunta_render::screen::removing_pack(&pack.to_string());
                    let same = vec![NextStep::PackRemove { pack: pack.clone() }];
                    let removal = Operation::Pack(PackOperation::Remove { pack });
                    Next::Confirm(Confirmation::new(asks, same, removal))
                }
                None => Next::Stays,
            },
            Stroke::Insert('n') => {
                let asks = yunta_render::screen::new_pack_asks();
                self.shown = PacksShown::Naming(LineAsked::new(asks));
                Next::Stays
            }
            Stroke::Insert('i') => {
                self.shown = PacksShown::Asking(PackAsked::source());
                Next::Stays
            }
            Stroke::Insert('u') => match self.pointed.clone() {
                Some(pack) => {
                    self.shown = PacksShown::Asking(PackAsked::reference(pack));
                    Next::Stays
                }
                None => Next::Stays,
            },
            Stroke::Decline => Next::Done,
            Stroke::Insert('q') | Stroke::Interrupt => Next::Leave,
            _ => Next::Stays,
        }
    }

    /// The pointer moved `by` rows, around at either end.
    fn moved(&mut self, by: isize) -> Next {
        let Some(listed) = &self.listed else {
            return Next::Stays;
        };
        let packs = listed.packs();
        if packs.is_empty() {
            return Next::Stays;
        }
        let at = packs
            .iter()
            .position(|pack| Some(*pack) == self.pointed.as_ref())
            .unwrap_or_default();
        let rows = packs.len() as isize;
        let to = (at as isize + by).rem_euclid(rows) as usize;
        self.pointed = packs.get(to).map(|pack| (*pack).clone());
        Next::Stays
    }

    /// Whether a new pack's name is being typed.
    pub(super) fn typing(&self) -> bool {
        match &self.shown {
            PacksShown::Naming(line) => line.typing(),
            PacksShown::Asking(asked) => asked.line().typing(),
            PacksShown::Fetched(_)
            | PacksShown::Fetching { .. }
            | PacksShown::Installing { .. }
            | PacksShown::Installed(_)
            | PacksShown::Updated { .. }
            | PacksShown::Listing
            | PacksShown::Auditing { .. }
            | PacksShown::Audited { .. }
            | PacksShown::Scaffolding { .. }
            | PacksShown::Scaffolded(_) => false,
        }
    }

    /// What work on the packs came to: the listing read, an audit, a pack removed
    /// (said, and the listing read again), or a new pack's page.
    pub(super) fn answered(&mut self, outcome: PackOutcome) -> Next {
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
        Next::Work(Operation::Pack(PackOperation::List))
    }

    /// What writing a new pack came to, where one was being written.
    fn scaffolded(&mut self, scaffolded: Arc<PackScaffolded>) {
        if let PacksShown::Scaffolding { .. } = self.shown {
            self.shown = PacksShown::Scaffolded(scaffolded);
        }
    }

    /// What the work under way says while it works.
    pub(super) fn remarked(&mut self, remark: Remark) {
        self.remarked.push(remark);
    }

    /// `said` said over the listing.
    pub(super) fn unanswered(&mut self, said: String) {
        self.said = Some(said);
        self.shown = PacksShown::Listing;
    }
}

/// The page `yunta pack new` prints of `scaffolded`, and what stopped its check
/// where something did.
fn scaffolded_page(scaffolded: &PackScaffolded, look: &Look) -> Doc {
    let page = NewPack {
        pack: &scaffolded.pack,
        at: &scaffolded.at,
        checked: scaffolded.checked.as_ref().ok(),
    };
    let doc = page.document(look);
    match &scaffolded.checked {
        Ok(_) => doc,
        Err(unchecked) => doc.with(Note(unchecked.clone())),
    }
}
