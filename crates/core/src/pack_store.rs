//! Where packs are installed. A store is one directory: each pack installed
//! in it sits under `packs/<publisher>/<name>/`, the lock that records them is
//! `yunta.lock` beside that directory, and a pack command that changes them
//! holds `yunta.lock.held` while it does. A project has its own store,
//! `.yunta` in its repository. A person has one more, the user state root
//! (`$YUNTA_HOME`, else `~/.yunta`), whose packs every project reads after
//! its own.
//!
//! [`PackStore`] is the one owner of that layout: a reader asks a store for a
//! path and never joins one itself. The alternative, each reader joining
//! `.yunta/packs` onto a repository, cannot say where a person's pack is, and
//! lets two readers disagree on where a pack lives. [`PackStores`] is the
//! order a project reads its stores in.
//!
//! A file every store keeps is one more method that joins its name onto the
//! store's directory; a store a project reads is one more in
//! [`PackStores::in_order`].

use std::path::{Path, PathBuf};

use crate::{user_state_root, Env, PackRef, Publisher};

/// Where a project keeps its store, relative to its repository.
const PROJECT_STORE: &str = ".yunta";
/// The directory of a store that holds its packs, one directory per
/// publisher.
const PACKS: &str = "packs";
/// The lock of a store, beside its packs.
const LOCK: &str = "yunta.lock";
/// The file a pack command holds from its read of the lock to its write.
const HOLD: &str = "yunta.lock.held";

/// Whose packs a store holds: a project's own, or a person's, which every
/// project reads after its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackScope {
    Project,
    User,
}

/// A directory packs are installed in: `packs/<publisher>/<name>/`, with the
/// lock beside them. Two stores are the same store when their scope and their
/// directory, as written, are the same; nothing resolves a link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackStore {
    scope: PackScope,
    dir: PathBuf,
}

impl PackStore {
    /// The store of the project whose repository is at `repo`: its `.yunta`.
    pub fn of_project(repo: &Path) -> Self {
        PackStore {
            scope: PackScope::Project,
            dir: repo.join(PROJECT_STORE),
        }
    }

    /// The user's store: the user state root `env` names. `None` when it
    /// names none.
    pub fn of_user(env: &Env) -> Option<Self> {
        user_state_root(env).map(|dir| PackStore {
            scope: PackScope::User,
            dir,
        })
    }

    /// Whose packs this store holds.
    pub fn scope(&self) -> PackScope {
        self.scope
    }

    /// The directory every pack of this store sits under.
    pub fn packs_dir(&self) -> PathBuf {
        self.dir.join(PACKS)
    }

    /// The directory the packs of `publisher` sit in.
    pub fn publisher_dir(&self, publisher: &Publisher) -> PathBuf {
        self.packs_dir().join(publisher.as_str())
    }

    /// The directory `pack` is installed in, whether or not it is there.
    pub fn pack_dir(&self, pack: &PackRef) -> PathBuf {
        self.publisher_dir(pack.publisher())
            .join(pack.name().as_str())
    }

    /// The lock that records what this store's packs were installed from.
    pub fn lock_file(&self) -> PathBuf {
        self.dir.join(LOCK)
    }

    /// The file a pack command holds while it changes this store.
    pub fn hold_file(&self) -> PathBuf {
        self.dir.join(HOLD)
    }

    /// The pack whose directory holds `path`, when `path` is under this
    /// store's packs and its first two segments there name a publisher and a
    /// pack. Compared as written.
    pub fn pack_holding(&self, path: &Path) -> Option<PackRef> {
        let packs = self.packs_dir();
        let mut inside = path.strip_prefix(&packs).ok()?.components();
        let mut segment = || inside.next()?.as_os_str().to_str();
        let publisher = segment()?.parse().ok()?;
        let name = segment()?.parse().ok()?;
        Some(PackRef::new(publisher, name))
    }
}

/// The stores a project reads packs from, in the order it reads them: its
/// own, then the user's.
///
/// Not the order a name is answered in, which starts with the repository's
/// own files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackStores {
    project: PackStore,
    user: Option<PackStore>,
}

impl PackStores {
    /// The stores the project in `repo` reads. `ambient` is the environment
    /// the user's store is found in; `None` reads no user store, as a run
    /// kept apart from the machine it runs on reads none. Joins paths and
    /// reads no disk.
    pub fn of(repo: &Path, ambient: Option<&Env>) -> Self {
        PackStores {
            project: PackStore::of_project(repo),
            user: ambient.and_then(PackStore::of_user),
        }
    }

    /// The project's own store.
    pub fn project(&self) -> &PackStore {
        &self.project
    }

    /// The user's store, when the environment names one.
    pub fn user(&self) -> Option<&PackStore> {
        self.user.as_ref()
    }

    /// The project's store, then the user's.
    pub fn in_order(&self) -> impl Iterator<Item = &PackStore> {
        std::iter::once(&self.project).chain(self.user.as_ref())
    }
}

#[cfg(test)]
mod tests;
