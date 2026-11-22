//! See [`super`]. The listing of one store's packs: this project's, or the user's,
//! which every project reads, and `g` turns from one to the other and reads it.
//! The pointer moves through the packs it lists, and the keys on it act on the one
//! pointed at, in the store it shows.

use std::sync::Arc;

use surco::components::Note;
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::step::{Arg, NextStep};
use yunta_core::{PackRef, PackScope};
use yunta_render::page::pack::{Listed, PointedPacks, Removed};
use yunta_render::page::Page;

use super::super::super::Act;
use super::super::Next;
use super::install::PackAsked;
use super::{PacksShown, PacksView};
use crate::dialog::Confirmation;
use crate::prompt::{LineAsked, Stroke};
use crate::{Operation, PackOperation};

impl PacksView {
    /// The pack removed last, then the packs pointed at, or that they are read.
    pub(super) fn listing(&self, look: &Look) -> Doc {
        let mut doc = Doc::new();
        if let Some(pack) = &self.removed {
            doc = doc.with_all(Removed { pack }.document(look).into_blocks());
        }
        let Some(listed) = &self.listed else {
            let reading = yunta_render::screen::reading_the_packs(self.scope).to_string();
            return doc.with(Note(reading));
        };
        let page = match &self.pointed {
            Some(pointed) => PointedPacks { listed, pointed }.document(look),
            None => listed.document(look),
        };
        doc.with_all(page.into_blocks())
    }

    /// What the packs offer on their listing, each with the key that takes it here:
    /// auditing, removing and updating the one pointed at, installing one, and the
    /// other store's packs. A new pack, whose command needs the name it is given, is
    /// offered by its key alone.
    pub(in super::super) fn offered(&self) -> Vec<Act> {
        if !matches!(self.shown, PacksShown::Listing) {
            return Vec::new();
        }
        let scope = self.scope;
        let pointed = self.pointed.iter().flat_map(|pack| {
            let pack = || pack.clone();
            [
                (
                    Stroke::Insert('a'),
                    NextStep::PackAudit {
                        pack: pack(),
                        scope,
                    },
                ),
                (
                    Stroke::Insert('x'),
                    NextStep::PackRemove {
                        pack: pack(),
                        scope,
                    },
                ),
                (
                    Stroke::Insert('u'),
                    NextStep::PackUpdate {
                        pack: pack(),
                        reference: Arg::Blank,
                        scope,
                    },
                ),
            ]
        });
        let source = Arg::Blank;
        let install = (Stroke::Insert('i'), NextStep::PackAdd { source, scope });
        let turn = NextStep::PackList {
            scope: other(scope),
        };
        pointed
            .chain([install, (Stroke::Insert('g'), turn)])
            .map(|(key, step)| Act::by(key, step))
            .collect()
    }

    /// What `stroke` asks for on the listing.
    pub(super) fn pressed_on_the_listing(&mut self, stroke: Stroke) -> Next {
        self.removed = None;
        self.said = None;
        let scope = self.scope;
        match stroke {
            Stroke::Down | Stroke::Tab | Stroke::Insert('j') => self.moved(1),
            Stroke::Up | Stroke::BackTab | Stroke::Insert('k') => self.moved(-1),
            Stroke::Insert('a') => match self.pointed.clone() {
                Some(pack) => {
                    self.shown = PacksShown::Auditing { pack: pack.clone() };
                    self.remarked.clear();
                    Next::Work(Operation::Pack(PackOperation::Audit { pack, scope }))
                }
                None => Next::Stays,
            },
            Stroke::Insert('x') => match self.pointed.clone() {
                Some(pack) => {
                    let asks = yunta_render::screen::removing_pack(&pack.to_string(), scope);
                    let same = vec![NextStep::PackRemove {
                        pack: pack.clone(),
                        scope,
                    }];
                    let removal = Operation::Pack(PackOperation::Remove { pack, scope });
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
            Stroke::Insert('g') => self.turned(),
            Stroke::Decline => Next::Done,
            Stroke::Insert('q') | Stroke::Interrupt => Next::Leave,
            _ => Next::Stays,
        }
    }

    /// The listing turned to the other store's packs, read anew: what it showed
    /// goes, and the pointer with it.
    fn turned(&mut self) -> Next {
        self.scope = other(self.scope);
        self.listed = None;
        self.pointed = None;
        Next::Work(self.read_again())
    }

    /// The work that reads the packs of the store the listing shows.
    pub(super) fn read_again(&self) -> Operation {
        Operation::Pack(PackOperation::List { scope: self.scope })
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

    /// The packs read, where they are the store the listing shows: a listing of the
    /// other, read before the listing turned, is dropped. The pointer stays on the
    /// one it stood on while it is installed, and goes to the first otherwise.
    pub(super) fn listed(&mut self, listed: Arc<Listed>) {
        if listed.scope() != self.scope {
            return;
        }
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

    /// `pack` removed: said over the listing, which is read again.
    pub(super) fn removed(&mut self, pack: PackRef) -> Next {
        self.removed = Some(pack);
        self.shown = PacksShown::Listing;
        Next::Work(self.read_again())
    }
}

/// The store a listing of `scope`'s packs turns to: the user's from this
/// project's, and this project's from the user's.
fn other(scope: PackScope) -> PackScope {
    match scope {
        PackScope::Project => PackScope::User,
        PackScope::User => PackScope::Project,
    }
}
