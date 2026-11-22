//! See [`crate`]. A repository's packs: vendored under `.yunta/packs/` and
//! recorded in `.yunta/yunta.lock`, fetched and installed, updated, removed,
//! listed, audited and scaffolded.
//!
//! A pack is installed in the order that keeps a person in charge: the
//! policy the config sets (`permissions.packs`: a publisher allow-list, and
//! `executors: allow|prompt|deny`) refuses first, then the audit is read for
//! a person, then their confirmation, then the pack is vendored through a
//! staging directory and recorded in the lock, written atomically. `prompt`,
//! the default, waits on a person's confirmation after the audit has shown
//! exactly what the executors are; `deny` refuses even with it; `allow`
//! installs without asking. An update is held to the same gates, since a new
//! ref is where new executor code first appears. Nothing of the pack runs
//! here, and a failure at any step leaves the repository as it was.
//!
//! Packs belong to the repository a workspace works in, not to a project's
//! state: listing, removing or scaffolding one reads no state root. What the
//! mechanics are is `vendor`; the policy is `policy`; what an installed pack
//! needs from the project is `needs`; a new pack's skeleton is `scaffold`; a
//! pack fetched, before it is installed, is `fetched`; the packs held from a
//! read of the lock to its write are `held`.

mod fetched;
mod held;
mod needs;
mod policy;
mod scaffold;
mod vendor;

use std::path::{Path, PathBuf};

use yunta_core::refusal::{PackRefusal, PackVerb};
use yunta_core::{
    CommitSha, ContentHash, PackAudit, PackLock, PackLockEntry, PackManifest, PackRef,
};
use yunta_render::page::pack::{Cases, Listed, Needs, VendoredPack, VendoredTree};
use yunta_render::refusal::Refused;

use crate::{Error, Failure, Project, Remarks, Stop, Workspace};
pub use fetched::FetchedPack;
use fetched::Installing;
use held::held;
use policy::PackPolicy;
use vendor::{PackError, Swap};

pub use scaffold::ScaffoldedPack;

impl Workspace {
    /// The packs of the repository this workspace works in.
    pub fn packs(&self) -> Packs<'_> {
        Packs { workspace: self }
    }
}

/// The packs of a repository: what it has vendored, and what can be
/// installed into it, updated, removed or scaffolded beside it.
pub struct Packs<'w> {
    workspace: &'w Workspace,
}

impl<'w> Packs<'w> {
    /// Fetches the pack `source` names, `<source>[@<ref>]`, to install it:
    /// cloned and its manifest read, then refused before anything is shown
    /// when it is installed already, its schema range leaves this binary
    /// out, its publisher is not allowed or its executors are denied.
    /// `confirmed` is the person's confirmation of what it executes, given
    /// ahead.
    pub async fn fetch(
        &self,
        source: &str,
        confirmed: bool,
        stop: &Stop,
    ) -> Result<FetchedPack, Error> {
        let (source, reference) = vendor::split_source_and_ref(source);
        let url = vendor::clone_url(source);
        let clone = clone_dir()?;
        let supervision = self.workspace.supervision(stop);
        let from = &self.workspace.dir;
        vendor::clone_pack(&url, reference, from, clone.path(), supervision)
            .await
            .map_err(failed)?;
        let manifest = vendor::read_manifest(clone.path()).map_err(failed)?;
        let pack = manifest.reference();
        refuse_vendored(&self.workspace.dir, &pack)?;
        // Policy first: a refused publisher or denied executors leave no
        // decision for a person to make, so the audit is never shown.
        policy::admit_schema(&manifest)?;
        let policy = PackPolicy::of(&self.workspace.dir, &self.workspace.env)?;
        policy.admit_publisher(&manifest.publisher)?;
        policy.refuse_denied(&manifest, PackVerb::Install)?;
        let installing = Installing::Added {
            source: url,
            reference: reference.map(str::to_string),
        };
        Ok(self.fetched(clone, manifest, policy, confirmed, installing))
    }

    /// Fetches `reference` of the installed `pack` to update it to: cloned
    /// from the source its lock records, and refused before anything is
    /// shown when the lock does not name it, the ref is another pack, its
    /// schema range leaves this binary out, its publisher is no longer
    /// allowed or its executors are denied.
    pub async fn fetch_update(
        &self,
        pack: &PackRef,
        reference: &str,
        confirmed: bool,
        stop: &Stop,
    ) -> Result<FetchedPack, Error> {
        let lock = vendor::load_lock(&self.workspace.dir).map_err(failed)?;
        let Some(entry) = lock.packs.get(pack).cloned() else {
            let pack = pack.clone();
            return Err(refused(PackRefusal::NotInstalled { pack }));
        };
        let clone = clone_dir()?;
        let supervision = self.workspace.supervision(stop);
        let from = &self.workspace.dir;
        vendor::clone_pack(
            &entry.source,
            Some(reference),
            from,
            clone.path(),
            supervision,
        )
        .await
        .map_err(failed)?;
        let manifest = vendor::read_manifest(clone.path()).map_err(failed)?;
        if manifest.reference() != *pack {
            return Err(refused(PackRefusal::AnotherPack {
                pack: pack.clone(),
                reference: reference.to_string(),
                cloned_from: entry.source,
                is: manifest.reference(),
            }));
        }
        // The same gates as an install: an allow-list narrowed since the
        // install stops pulling from a publisher it no longer trusts.
        policy::admit_schema(&manifest)?;
        let policy = PackPolicy::of(&self.workspace.dir, &self.workspace.env)?;
        policy.admit_publisher(pack.publisher())?;
        policy.refuse_denied(&manifest, PackVerb::Update)?;
        let installing = Installing::Updated {
            source: entry.source,
            reference: reference.to_string(),
        };
        Ok(self.fetched(clone, manifest, policy, confirmed, installing))
    }

    /// Removes the installed `pack`: its lock entry first, then its tree,
    /// moved aside and deleted only once the lock without it is written, so
    /// a failed write puts it back. The packs are held from the read of the
    /// lock to its write. A tree that stays on disk is said through
    /// `remarks`.
    pub async fn remove(
        &self,
        pack: &PackRef,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<(), Error> {
        let dir = &self.workspace.dir;
        let _held = held(self.workspace, stop).await?;
        let mut lock = vendor::load_lock(dir).map_err(failed)?;
        if lock.packs.remove(pack).is_none() {
            let pack = pack.clone();
            return Err(refused(PackRefusal::NothingToRemove { pack }));
        }
        let aside = Swap::aside(&vendor::vendor_dir(dir, pack)).map_err(failed)?;
        if let Err(cause) = vendor::save_lock(dir, &lock) {
            let undone = aside.map_or(Ok(()), Swap::undo);
            return Err(failed(vendor::with_cleanup(cause, undone)));
        }
        if let Some(swap) = aside {
            swap.commit(remarks);
        }
        Ok(())
    }

    /// The packs the lock holds, each as its vendored tree stands against
    /// the content the lock recorded, hashed again rather than trusted; or
    /// that none is installed.
    pub fn list(&self) -> Result<Listed, Error> {
        let dir = &self.workspace.dir;
        let lock = vendor::load_lock(dir).map_err(failed)?;
        let shown = |path: &Path| surco::paths::shown(path, dir, None);
        if lock.packs.is_empty() {
            let looked_in = shown(&vendor::packs_root(dir));
            return Ok(Listed::Empty { looked_in });
        }
        let packs = lock
            .packs
            .iter()
            .map(|(pack, entry)| VendoredPack {
                pack: pack.clone(),
                locked: entry.clone(),
                tree: vendored_tree(&vendor::vendor_dir(dir, pack), entry),
            })
            .collect();
        let lock = shown(&vendor::lock_path(dir));
        Ok(Listed::Locked { packs, lock })
    }

    /// The installed `pack`'s audit, read, never run: every command, context
    /// source, permission, agent, `mcp` server, executor and path a scope
    /// names, and each workflow's whole prompt; then whether it ships cases
    /// of its own, each run on it. Refused when it is not vendored here.
    /// What its cases' runs say is said through `remarks`.
    pub async fn audit(
        &self,
        pack: &PackRef,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<AuditedPack, Error> {
        let dir = &self.workspace.dir;
        let pack_dir = vendor::vendor_dir(dir, pack);
        if !pack_dir.is_dir() {
            let root = vendor::packs_root(dir);
            let pack = pack.clone();
            return Err(refused(PackRefusal::NotVendored { pack, root }));
        }
        let manifest = vendor::read_manifest(&pack_dir).map_err(failed)?;
        let audit = yunta_engine::audit_pack(&pack_dir, manifest);
        let cases = self.workspace.cases_run(&pack_dir, remarks, stop).await;
        Ok(AuditedPack { audit, cases })
    }

    /// A fetched pack, ready to be audited by a person and installed.
    fn fetched(
        &self,
        clone: tempfile::TempDir,
        manifest: PackManifest,
        policy: PackPolicy,
        confirmed: bool,
        installing: Installing,
    ) -> FetchedPack {
        let workspace = self.workspace.clone();
        FetchedPack::new(workspace, clone, manifest, policy, confirmed, installing)
    }
}

/// Refused when `pack` is vendored in the repository at `dir` already: what
/// an add is judged on when it is fetched, and again once the packs are held.
fn refuse_vendored(dir: &Path, pack: &PackRef) -> Result<(), Error> {
    let at = vendor::vendor_dir(dir, pack);
    match at.exists() {
        true => Err(refused(PackRefusal::AlreadyInstalled {
            pack: pack.clone(),
            at,
        })),
        false => Ok(()),
    }
}

/// A pack vendored in the repository and recorded in its lock.
pub struct InstalledPack {
    workspace: Workspace,
    pub pack: PackRef,
    pub manifest: PackManifest,
    /// The commit its tree was vendored from.
    pub commit: CommitSha,
    /// The ref the lock records it at.
    pub reference: String,
    /// Where it is vendored.
    pub dir: PathBuf,
}

impl InstalledPack {
    /// The pack's own cases, counted, none of them run: what an install
    /// says when nothing of the pack is to run.
    pub fn shipped_cases(&self) -> Cases {
        crate::cases::shipped(&self.dir)
    }

    /// The pack's own cases, each run on its vendored copy, after the
    /// install and never as part of deciding it. What their runs say is said
    /// through `remarks`.
    pub async fn run_cases(&self, remarks: &dyn Remarks, stop: &Stop) -> Cases {
        self.workspace.cases_run(&self.dir, remarks, stop).await
    }

    /// What the pack's workflows need from `project`, the one it was
    /// installed in, that its config does not declare, each once, and what
    /// its repository was detected to have.
    pub async fn needs(&self, project: &Project, stop: &Stop) -> Result<Needs, Error> {
        needs::unmet(project, &self.dir, &self.manifest, stop).await
    }
}

/// An installed pack's audit, and what its own cases did.
#[derive(Debug)]
pub struct AuditedPack {
    pub audit: PackAudit,
    pub cases: Cases,
}

/// Vendors the clone at `clone` as `pack` under `dir` through a staging
/// directory and records it in `lock` with the entry `entry` makes of its
/// content hash, the lock written atomically. A tree that cannot be vendored
/// leaves the installed pack and the lock exactly as they were, and a failure
/// after the swap puts the installed pack back. Answers where the pack is.
fn vendor_and_lock(
    dir: &Path,
    clone: &Path,
    pack: &PackRef,
    mut lock: PackLock,
    entry: impl FnOnce(ContentHash) -> PackLockEntry,
    remarks: &dyn Remarks,
) -> Result<PathBuf, PackError> {
    let at = vendor::vendor_dir(dir, pack);
    let staging = vendor::staging_dir(dir, pack);
    let swap = vendor::vendor_into_place(clone, &staging, &at)?;
    let content_hash = match vendor::hash_tree(&at) {
        Ok(hash) => hash,
        Err(cause) => return Err(vendor::with_cleanup(cause, swap.undo())),
    };
    lock.packs.insert(pack.clone(), entry(content_hash));
    if let Err(cause) = vendor::save_lock(dir, &lock) {
        return Err(vendor::with_cleanup(cause, swap.undo()));
    }
    swap.commit(remarks);
    Ok(at)
}

/// How the vendored tree at `at` stands against its lock `entry`: whether
/// its files still hash to the recorded content, or why they cannot be read.
fn vendored_tree(at: &Path, entry: &PackLockEntry) -> VendoredTree {
    match vendor::hash_tree(at) {
        Ok(hash) if hash == entry.content_hash => VendoredTree::AsLocked,
        Ok(_) => VendoredTree::Modified,
        // Only a directory that is not there counts as missing.
        Err(PackError::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            VendoredTree::Missing
        }
        Err(error) => VendoredTree::Unreadable(yunta_core::describe(&error)),
    }
}

/// A fresh, empty directory a pack is cloned into.
fn clone_dir() -> Result<tempfile::TempDir, Error> {
    tempfile::tempdir().map_err(|source| Failure::new(CloneDirUncreated(source)).into())
}

/// A pack command `refusal` refuses, as the API returns it.
fn refused(refusal: PackRefusal) -> Error {
    Refused::Pack(Box::new(refusal)).into()
}

/// What failed fetching, vendoring or recording a pack, below the API; a git
/// the caller's stop cancelled is the stop, as every git the API asks is.
fn failed(error: PackError) -> Error {
    match error {
        PackError::Git(git) => git.into(),
        error => Failure::new(error).into(),
    }
}

/// The directory a pack is cloned into could not be made.
#[derive(Debug, thiserror::Error)]
#[error("failed to create a temp directory to clone into: {0}")]
struct CloneDirUncreated(#[source] std::io::Error);
