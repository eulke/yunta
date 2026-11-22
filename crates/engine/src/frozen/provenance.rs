//! Which pack a workflow came from, and the pack files whose provenance
//! could not be read. A birth reads it from the tree it is born in: for the
//! root workflow, and for each child when the walk reaches it, so a child's
//! provenance is frozen with the child rather than read again at its birth.
//! A pack's version is its own manifest's and its commit is the one its own
//! store's lock pins, whichever store the pack is installed in.

use std::fmt::Display;
use std::path::Path;

use yunta_core::{CommitSha, Env, PackProvenance, PackRef, PackStore, ProvenanceGap};

use crate::catalog::WorkflowOrigin;

/// Which pack (and exactly which version) the workflow in `workflow_dir`
/// came from, if any. Provenance is for `yunta status`/`receipt`, never
/// load-bearing for the run's own correctness: that guarantee comes from
/// the workflow itself being embedded whole, and from `workflow_hash`
/// covering it. `None` for a repo-origin workflow, and whenever the pack's
/// own manifest cannot be read at this moment; the file that could not be
/// read is then a gap, with its reason, and the run is born all the same.
/// `ambient` is the environment the user's store is found in.
pub(crate) async fn pack_provenance(
    repo: &Path,
    ambient: Option<&Env>,
    workflow_dir: &Path,
) -> (Option<PackProvenance>, Vec<ProvenanceGap>) {
    let origin = crate::catalog::origin_of(repo, ambient, workflow_dir);
    let (
        Some(manifest_file),
        WorkflowOrigin::Pack {
            publisher,
            pack_name,
            store,
        },
    ) = (origin.manifest_file(), origin)
    else {
        return (None, Vec::new());
    };
    let manifest = match read_pack_manifest(repo, &manifest_file).await {
        Ok(manifest) => manifest,
        Err(unread) => return (None, vec![unread]),
    };
    let pack = PackRef::new(publisher.clone(), pack_name.clone());
    let (commit, lock_gap) = locked_commit(repo, &store, &pack).await;
    let provenance = PackProvenance {
        publisher,
        name: pack_name,
        version: manifest.version,
        commit,
    };
    (Some(provenance), lock_gap.into_iter().collect())
}

/// The pack's manifest as it reads, or the gap that says why it does not.
async fn read_pack_manifest(
    repo: &Path,
    file: &Path,
) -> Result<yunta_core::PackManifest, ProvenanceGap> {
    let text = tokio::fs::read_to_string(file)
        .await
        .map_err(|error| gap(repo, file, &error))?;
    yunta_core::PackManifest::read(&text).map_err(|error| gap(repo, file, &error))
}

/// The commit the lock of `store` pins `pack` to. A store with no lock says
/// nothing about commits, which is not a gap; a lock that cannot be read, or
/// one a newer binary wrote, is.
async fn locked_commit(
    repo: &Path,
    store: &PackStore,
    pack: &PackRef,
) -> (Option<CommitSha>, Option<ProvenanceGap>) {
    let file = store.lock_file();
    let text = match tokio::fs::read_to_string(&file).await {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (None, None),
        Err(error) => return (None, Some(gap(repo, &file, &error))),
    };
    match yunta_core::PackLock::read(text.as_bytes()) {
        Ok(lock) => {
            let commit = lock.doc.packs.get(pack).map(|entry| entry.commit.clone());
            (commit, None)
        }
        Err(error) => (None, Some(gap(repo, &file, &error))),
    }
}

/// A file whose provenance could not be read, with the reason it gave: named
/// by its path within `repo` when it is inside it, as a project's pack is,
/// else in full, as a user's pack is.
fn gap(repo: &Path, file: &Path, detail: &impl Display) -> ProvenanceGap {
    ProvenanceGap {
        file: file
            .strip_prefix(repo)
            .unwrap_or(file)
            .display()
            .to_string(),
        detail: detail.to_string(),
    }
}
