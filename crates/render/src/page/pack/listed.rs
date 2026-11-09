//! The packs a project has installed, as `pack list` says them: each as its
//! vendored tree stands against the content its lock records, and where the
//! lock is; or where none is.

use surco::components::{Checklist, Fields, Found, Furrow, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::{PackLockEntry, PackRef};

use crate::page::Page;

/// The packs a project has installed, as `pack list` says them.
pub enum Listed {
    /// No pack is installed under `looked_in`, as a reader in the project
    /// names it.
    Empty { looked_in: String },
    /// Each pack the lock holds, as its vendored tree stands against the
    /// lock, and where the lock is, as a reader in the project names it.
    Locked {
        packs: Vec<VendoredPack>,
        lock: String,
    },
}

/// One installed pack, as `pack list` reads it: what its lock records, and
/// how its vendored tree stands against it.
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

impl Page for Listed {
    fn document(&self, _look: &Look) -> Doc {
        match self {
            Listed::Empty { looked_in } => Doc::new()
                .with(Headline::new(Mark::Absent, "packs", "none installed"))
                .with(Furrow(Vec::new()))
                .with(Fields::new().push_if("looked in", looked_in.as_str())),
            Listed::Locked { packs, lock } => {
                let mut checks = Checklist::default();
                for vendored in packs {
                    checks.push(vendored.found(), vendored.pack.to_string(), vendored.said());
                }
                let mark = match checks.holds() {
                    true => Mark::Done,
                    false => Mark::Failed,
                };
                let installed = Headline::new(mark, "packs", format!("{} installed", packs.len()))
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
