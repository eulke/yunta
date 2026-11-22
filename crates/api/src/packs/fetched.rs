//! See [`super`]. A pack cloned and read, not yet vendored: its audit, what a
//! person confirms, and its install from that very copy, which never fetches
//! again. What it is installed as, a pack new to the repository or an update,
//! is `Installing`.

use std::path::Path;

use yunta_core::refusal::{PackRefusal, PackVerb};
use yunta_core::{PackAudit, PackLock, PackLockEntry, PackManifest, PackRef};

use super::policy::PackPolicy;
use super::{failed, held, refuse_vendored, refused, vendor, vendor_and_lock, InstalledPack};
use crate::{Error, Remarks, Stop, Workspace};

/// A pack cloned and read, not yet vendored: what a person reads before it
/// is installed.
///
/// Not an installed pack: nothing of it is in the repository until
/// [`FetchedPack::install`].
pub struct FetchedPack {
    workspace: Workspace,
    clone: tempfile::TempDir,
    pub manifest: PackManifest,
    policy: PackPolicy,
    confirmed: bool,
    installing: Installing,
}

/// What a fetched pack is installed as.
pub(super) enum Installing {
    /// A pack new to the repository, from `source`, at `reference` when one
    /// was asked for, else on the branch its clone lands on.
    Added {
        source: String,
        reference: Option<String>,
    },
    /// An installed pack moved to `reference`, from the `source` its lock
    /// entry records.
    Updated { source: String, reference: String },
}

impl Installing {
    /// What the install is asked to do, as a refusal names it.
    fn verb(&self) -> PackVerb {
        match self {
            Installing::Added { .. } => PackVerb::Install,
            Installing::Updated { .. } => PackVerb::Update,
        }
    }

    /// Refused when the repository, as `lock` read with the packs held and
    /// the trees vendored under `dir` have it now, no longer admits this
    /// install: an add of a pack vendored since, or an update of one no
    /// longer locked.
    fn still_admitted(&self, pack: &PackRef, lock: &PackLock, dir: &Path) -> Result<(), Error> {
        match self {
            Installing::Added { .. } => refuse_vendored(dir, pack),
            Installing::Updated { .. } if lock.packs.contains_key(pack) => Ok(()),
            Installing::Updated { .. } => {
                Err(refused(PackRefusal::NotInstalled { pack: pack.clone() }))
            }
        }
    }
}

impl FetchedPack {
    /// The pack cloned at `clone`, its manifest read, judged under `policy` with
    /// what it executes `confirmed` ahead or not, to be installed as `installing`
    /// says, in `workspace`'s repository.
    pub(super) fn new(
        workspace: Workspace,
        clone: tempfile::TempDir,
        manifest: PackManifest,
        policy: PackPolicy,
        confirmed: bool,
        installing: Installing,
    ) -> Self {
        FetchedPack {
            workspace,
            clone,
            manifest,
            policy,
            confirmed,
            installing,
        }
    }

    /// This fetch with what it executes confirmed by a person who read its
    /// audit: what `--yes` gives ahead on the command line.
    pub fn confirmed(mut self) -> Self {
        self.confirmed = true;
        self
    }

    /// The ref an update moves the pack to; none for a pack new to the
    /// repository.
    pub fn updating_to(&self) -> Option<&str> {
        match &self.installing {
            Installing::Updated { reference, .. } => Some(reference),
            Installing::Added { .. } => None,
        }
    }

    /// Where the copy its audit reads stands: what it installs from, gone once
    /// it is installed or let go.
    pub fn checkout(&self) -> &Path {
        self.clone.path()
    }

    /// The pack's audit, read, never run: the inventory a person confirms
    /// against.
    pub fn audit(&self) -> PackAudit {
        yunta_engine::audit_pack(self.clone.path(), self.manifest.clone())
    }

    /// Whether the executors the pack declares wait on a person's
    /// confirmation, which was not given ahead.
    pub fn waits_on_yes(&self) -> bool {
        self.policy.waits_on_yes(&self.manifest, self.confirmed)
    }

    /// Installs the pack: refused when the executors it declares wait on a
    /// confirmation not given, or when, with the packs held, another pack
    /// command has vendored it since an add was fetched or removed it since
    /// an update was; then, with the packs still held, vendored through a
    /// staging directory beside its place and recorded in the lock, read
    /// again and written atomically. A failure leaves the installed tree and
    /// the lock as they were. A replaced tree that stays on disk is said
    /// through `remarks`.
    pub async fn install(self, remarks: &dyn Remarks, stop: &Stop) -> Result<InstalledPack, Error> {
        self.policy
            .admit_executors(&self.manifest, self.confirmed, self.installing.verb())?;
        let supervision = self.workspace.supervision(stop);
        let clone = self.clone.path();
        let commit = vendor::head_commit(clone, supervision)
            .await
            .map_err(failed)?;
        let (source, reference) = match &self.installing {
            Installing::Added {
                source,
                reference: Some(reference),
            } => (source.clone(), reference.clone()),
            Installing::Added {
                source,
                reference: None,
            } => {
                let branch = vendor::current_branch(clone, supervision).await;
                (source.clone(), branch.map_err(failed)?)
            }
            Installing::Updated { source, reference } => (source.clone(), reference.clone()),
        };
        let dir = &self.workspace.dir;
        // Held from the read of the lock to its write, so an entry another
        // pack command writes meanwhile is read, never written over. Read
        // before anything is written: a lock that cannot be read stops the
        // install before it starts.
        let _held = held(&self.workspace, stop).await?;
        let lock = vendor::load_lock(dir).map_err(failed)?;
        let pack = self.manifest.reference();
        // The fetch judged the repository before the packs were held; another
        // pack command may have installed or removed this pack since.
        self.installing.still_admitted(&pack, &lock, dir)?;
        let entry = |content_hash| PackLockEntry {
            publisher: pack.publisher().clone(),
            name: pack.name().clone(),
            source,
            r#ref: reference.clone(),
            commit: commit.clone(),
            content_hash,
        };
        let at = vendor_and_lock(dir, clone, &pack, lock, entry, remarks).map_err(failed)?;
        Ok(InstalledPack {
            workspace: self.workspace,
            pack,
            manifest: self.manifest,
            commit,
            reference,
            dir: at,
        })
    }
}
