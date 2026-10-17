//! Which pack a workflow came from, and the pack files whose provenance
//! could not be read. A birth reads it from the tree it is born in: for the
//! root workflow, and for each child when the walk reaches it, so a child's
//! provenance is frozen with the child rather than read again at its birth.

use std::fmt::Display;
use std::path::Path;

use yunta_core::{CommitSha, PackName, PackProvenance, PackRef, ProvenanceGap, Publisher};

/// Which pack (and exactly which version) the workflow in `workflow_dir`
/// came from, if any. Provenance is for `yunta status`/`receipt`, never
/// load-bearing for the run's own correctness: that guarantee comes from
/// the workflow itself being embedded whole, and from `workflow_hash`
/// covering it. `None` for a repo-origin workflow, and whenever the pack's
/// own manifest cannot be read at this moment; the file that could not be
/// read is then a gap, with its reason, and the run is born all the same.
pub(crate) async fn pack_provenance(
    repo: &Path,
    workflow_dir: &Path,
) -> (Option<PackProvenance>, Vec<ProvenanceGap>) {
    let crate::catalog::WorkflowOrigin::Pack {
        publisher,
        pack_name,
    } = crate::catalog::origin_of(repo, workflow_dir)
    else {
        return (None, Vec::new());
    };
    let pack_file = Path::new(".yunta/packs")
        .join(publisher.as_str())
        .join(pack_name.as_str())
        .join("pack.yaml");
    let manifest = match read_pack_manifest(repo, &pack_file).await {
        Ok(manifest) => manifest,
        Err(unread) => return (None, vec![unread]),
    };
    let (commit, lock_gap) = locked_commit(repo, &publisher, &pack_name).await;
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
    let text = tokio::fs::read_to_string(repo.join(file))
        .await
        .map_err(|error| gap(file, &error))?;
    yunta_core::yaml::parse::<yunta_core::PackManifest>(&text).map_err(|error| gap(file, &error))
}

/// The commit the lock pins the pack to. A repository with no lock says
/// nothing about commits, which is not a gap; a lock that cannot be read is.
async fn locked_commit(
    repo: &Path,
    publisher: &Publisher,
    pack_name: &PackName,
) -> (Option<CommitSha>, Option<ProvenanceGap>) {
    let file = Path::new(".yunta/yunta.lock");
    let text = match tokio::fs::read_to_string(repo.join(file)).await {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (None, None),
        Err(error) => return (None, Some(gap(file, &error))),
    };
    match yunta_core::yaml::parse::<yunta_core::PackLock>(&text) {
        Ok(lock) => {
            let commit = lock
                .packs
                .get(&PackRef::new(publisher.clone(), pack_name.clone()))
                .map(|entry| entry.commit.clone());
            (commit, None)
        }
        Err(error) => (None, Some(gap(file, &error))),
    }
}

/// A file whose provenance could not be read, named by its repo-relative
/// path, with the reason it gave.
fn gap(file: &Path, detail: &impl Display) -> ProvenanceGap {
    ProvenanceGap {
        file: file.display().to_string(),
        detail: detail.to_string(),
    }
}
