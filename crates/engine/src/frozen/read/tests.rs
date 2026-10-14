//! What a frozen skill's copy answers: the copy holds exactly the files the
//! run froze, or nothing mounts from it.

use std::path::{Path, PathBuf};

use yunta_core::{ConfigLayer, Frozen};

use super::Source;
use crate::frozen::{store, walk, FrozenError};

async fn write(path: &Path, text: &str) {
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(path, text).await.unwrap();
}

/// Freezes a repo whose one prompt node mounts the `notes` skill, and writes
/// the run that froze it. Returns the frozen set and the run's directory.
async fn run_freezing_notes(root: &Path) -> (Frozen, PathBuf) {
    let repo = root.join("repo");
    let run = root.join("run");
    write(&repo.join(".yunta/skills/notes/SKILL.md"), "Take notes.\n").await;
    let workflow = yunta_core::workflow::read::read(
        "name: frozen\nnodes:\n  - { id: audit, kind: prompt, runner: executor, skills: [notes], prompt: \"x\" }\n",
        &repo.join("wf.yaml"),
    )
    .unwrap();
    let (frozen, blobs) = walk::freeze(&repo, &workflow, &ConfigLayer::default(), None)
        .await
        .unwrap();
    store::write_at_birth(&run, &blobs, Some(&frozen))
        .await
        .unwrap();
    (frozen, run)
}

#[tokio::test]
async fn a_file_added_to_a_frozen_skill_copy_is_refused_before_it_mounts() {
    let root = tempfile::tempdir().unwrap();
    let (frozen, run) = run_freezing_notes(root.path()).await;
    let source = Source::Frozen {
        frozen: &frozen,
        run_dir: &run,
    };
    let skill = "notes".parse().unwrap();
    let dir = source.skill_dir(&skill).await.unwrap();

    write(&dir.join("extra.md"), "Added after the freeze.\n").await;
    let refused = source.skill_dir(&skill).await;
    assert!(
        matches!(&refused, Err(FrozenError::Mismatch { input }) if input == "skills/notes"),
        "{refused:?}"
    );

    // With the extra file gone the copy holds the files the run froze again,
    // so the refusal was about that file alone.
    tokio::fs::remove_file(dir.join("extra.md")).await.unwrap();
    assert!(source.skill_dir(&skill).await.is_ok());
}

#[tokio::test]
async fn a_symlink_in_a_frozen_skill_copy_is_refused_before_it_mounts() {
    let root = tempfile::tempdir().unwrap();
    let (frozen, run) = run_freezing_notes(root.path()).await;
    let source = Source::Frozen {
        frozen: &frozen,
        run_dir: &run,
    };
    let skill = "notes".parse().unwrap();
    let dir = source.skill_dir(&skill).await.unwrap();

    std::os::unix::fs::symlink(dir.join("SKILL.md"), dir.join("alias.md")).unwrap();
    let refused = source.skill_dir(&skill).await;
    assert!(
        matches!(&refused, Err(FrozenError::Mismatch { input }) if input == "skills/notes"),
        "{refused:?}"
    );
}
