//! What a reader finds under a run's directory: where the bytes of what the run
//! accepted sit, by hash, and where the view of each artifact is written.
//!
//! The engine writes these places and every surface that names one reads it
//! from here, so a page that points a person at a file and the store that wrote
//! it cannot disagree. The engine's own working space under the directory —
//! scratch, a node's staging, a unit's checkout — is no reader's, and stays the
//! engine's (`yunta_engine::run_dir`).

use std::path::{Path, PathBuf};

use crate::events::Shown;
use crate::{ContentHash, NodeId};

/// The run directory's own name for where artifacts live. Every path a
/// run records for an artifact is relative to the run directory and
/// starts here, which is the shape the log, the diagnostics and the run
/// contract all use — so writing one and reading one back read the same
/// name.
pub const ARTIFACTS_DIR: &str = "artifacts";

/// The run directory that holds every artifact's bytes, by hash: the store
/// writes and reads them under this name, and what says where they are
/// names it the same way.
pub const OBJECTS_DIR: &str = "objects";

/// Where the bytes `hash` names live, under the run rooted at `run_dir`.
pub fn object(run_dir: &Path, hash: &ContentHash) -> PathBuf {
    run_dir.join(OBJECTS_DIR).join(hash.as_str())
}

/// Where the view of one artifact goes, relative to the run directory: a
/// producer's artifacts under its node, what the run acquired without a
/// producer at the root.
pub fn view(node: Option<&NodeId>, name: &str) -> PathBuf {
    let mut path = PathBuf::from(ARTIFACTS_DIR);
    if let Some(node) = node {
        path.push(node.as_str());
    }
    path.push(name);
    path
}

/// Where the document an escalation shows sits, relative to the run
/// directory: the view of its canonical bytes, as the engine reads it.
pub fn view_of(shown: &Shown) -> PathBuf {
    view(shown.producer.as_ref(), &shown.artifact.view_name())
}

/// The run's view of what it holds, under the run rooted at `run_dir`: one
/// file per artifact, written from the acceptance that named it.
pub fn artifacts(run_dir: &Path) -> PathBuf {
    run_dir.join(ARTIFACTS_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "4f6c0d3a1b2e9f8a7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b";

    #[test]
    fn an_object_sits_under_objects_by_its_hash() {
        let hash: ContentHash = HASH.parse().expect("a content hash");
        assert_eq!(
            object(Path::new("/runs/R"), &hash),
            PathBuf::from(format!("/runs/R/objects/{HASH}"))
        );
    }

    #[test]
    fn a_view_sits_under_its_producer_and_an_unproduced_one_at_the_root() {
        let node = NodeId::from("plan");
        assert_eq!(
            view(Some(&node), "tasks.yaml"),
            PathBuf::from("artifacts/plan/tasks.yaml")
        );
        assert_eq!(view(None, "notes.md"), PathBuf::from("artifacts/notes.md"));
    }

    #[test]
    fn the_view_of_a_run_is_its_artifacts_folder() {
        assert_eq!(
            artifacts(Path::new("/runs/R")),
            PathBuf::from("/runs/R/artifacts")
        );
    }
}
