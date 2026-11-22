//! The packs a store holds, as `pack list` says them: each as its vendored
//! tree stands against the content its lock records, and where the lock is; or
//! where none is. Its headline says whose packs they are: this project's are
//! `packs`, and the user's, which every project reads, `the user's packs`. The
//! application moves through them with one pointed at: [`PointedPacks`], the
//! same rows with the pointer on one.

use surco::components::{Checklist, Fields, Found, Furrow, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::{PackLockEntry, PackRef, PackScope};

use crate::page::Page;

/// The packs the store `scope` names holds, as `pack list` says them.
#[derive(Debug)]
pub enum Listed {
    /// No pack is installed under `looked_in`, as a reader in the project
    /// names it.
    Empty { scope: PackScope, looked_in: String },
    /// Each pack the lock holds, as its vendored tree stands against the
    /// lock, and where the lock is, as a reader in the project names it.
    Locked {
        scope: PackScope,
        packs: Vec<VendoredPack>,
        lock: String,
    },
}

/// One installed pack, as `pack list` reads it: what its lock records, and
/// how its vendored tree stands against it.
#[derive(Debug)]
pub struct VendoredPack {
    pub pack: PackRef,
    /// The ref and the commit the lock records it at, and the content it
    /// hashed to.
    pub locked: PackLockEntry,
    pub tree: VendoredTree,
}

/// How a vendored pack's files stand against the content its lock records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VendoredTree {
    /// They hash to it.
    AsLocked,
    /// They no longer do.
    Modified,
    /// Its directory is not there.
    Missing,
    /// They cannot be read, for this.
    Unreadable(String),
}

impl VendoredPack {
    /// What its row finds: whether its tree holds to its lock.
    fn found(&self) -> Found {
        match self.tree {
            VendoredTree::AsLocked => Found::Holds,
            VendoredTree::Modified => Found::Caution,
            VendoredTree::Missing | VendoredTree::Unreadable(_) => Found::Problem,
        }
    }

    /// What its row says: the ref and commit its lock records, and how its
    /// tree stands against them.
    fn said(&self) -> String {
        let installed = format!(
            "{} ({})",
            self.locked.r#ref,
            self.locked.commit.abbreviated()
        );
        match &self.tree {
            VendoredTree::AsLocked => format!("{installed}, as its lock records it"),
            VendoredTree::Modified => {
                format!("{installed}, modified: its files no longer match its lock")
            }
            VendoredTree::Missing => format!("{installed}, missing: its directory is not there"),
            VendoredTree::Unreadable(why) => format!("{installed}, cannot be read: {why}"),
        }
    }
}

/// The packs installed as a person moves through them at a screen: the rows
/// `pack list` prints, the pointer on the one `pointed` names. Not [`Listed`]
/// read whole, which carries no pointer.
pub struct PointedPacks<'a> {
    pub listed: &'a Listed,
    pub pointed: &'a PackRef,
}

impl Page for Listed {
    fn document(&self, _look: &Look) -> Doc {
        self.drawn(None)
    }
}

impl Page for PointedPacks<'_> {
    fn document(&self, _look: &Look) -> Doc {
        self.listed.drawn(Some(self.pointed))
    }
}

impl Listed {
    /// Whose packs it lists: the store it read.
    pub fn scope(&self) -> PackScope {
        match self {
            Listed::Empty { scope, .. } | Listed::Locked { scope, .. } => *scope,
        }
    }

    /// The packs it lists, in its order: what a pointer moves through.
    pub fn packs(&self) -> Vec<&PackRef> {
        match self {
            Listed::Empty { .. } => Vec::new(),
            Listed::Locked { packs, .. } => packs.iter().map(|vendored| &vendored.pack).collect(),
        }
    }

    /// The page, the pointer on the row of `pointed` where it names one.
    fn drawn(&self, pointed: Option<&PackRef>) -> Doc {
        let whose = whose_packs(self.scope());
        match self {
            Listed::Empty { looked_in, .. } => Doc::new()
                .with(Headline::new(Mark::Absent, whose, "none installed"))
                .with(Furrow(Vec::new()))
                .with(Fields::new().push_if("looked in", looked_in.as_str())),
            Listed::Locked { packs, lock, .. } => {
                let mut checks = Checklist::default();
                if let Some(pointed) = pointed {
                    checks = checks.pointing_at(pointed.to_string());
                }
                for vendored in packs {
                    checks.push(vendored.found(), vendored.pack.to_string(), vendored.said());
                }
                let mark = match checks.holds() {
                    true => Mark::Done,
                    false => Mark::Failed,
                };
                let installed = Headline::new(mark, whose, format!("{} installed", packs.len()))
                    .tally(checks.count(Found::Problem), checks.count(Found::Caution));
                Doc::new()
                    .with(installed)
                    .with(Furrow(Vec::new()))
                    .with(checks)
                    .with(Fields::new().push_if("lock", lock.as_str()))
            }
        }
    }
}

/// Whose packs a listing's headline names: this project's are `packs`, the
/// user's, which every project reads, `the user's packs`.
fn whose_packs(scope: PackScope) -> &'static str {
    match scope {
        PackScope::Project => "packs",
        PackScope::User => "the user's packs",
    }
}
