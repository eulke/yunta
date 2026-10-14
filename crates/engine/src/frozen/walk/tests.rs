//! The walk's tests: what a freeze refuses, and what a frozen set hands back.

use super::*;
use crate::frozen::read::{KnowledgeFile, Source};
use crate::frozen::store;

async fn write(path: &Path, contents: impl AsRef<[u8]>) {
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(path, contents).await.unwrap();
}

fn workflow(text: &str, repo: &Path) -> Workflow {
    yunta_core::workflow::read::read(text, &repo.join("wf.yaml")).unwrap()
}

/// The freeze of a workflow whose one node runs as the agent `reviewer`, with
/// the agent's file holding `contents`.
async fn freeze_reviewer(
    contents: impl AsRef<[u8]>,
) -> Result<(Frozen, Vec<(ContentHash, Vec<u8>)>), FrozenError> {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path();
    write(&repo.join(".yunta/agents/reviewer.md"), contents).await;
    let parent = workflow(
        "name: parent\nnodes:\n  - { id: audit, kind: prompt, runner: executor, agent: reviewer, prompt: \"x\" }\n",
        repo,
    );
    freeze(repo, &parent, &ConfigLayer::default(), None).await
}

#[tokio::test]
async fn a_pack_path_that_leaves_its_pack_is_refused_at_freeze() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path();
    write(
        &repo.join(".yunta/packs/pub/tools/pack.yaml"),
        "publisher: pub\nname: tools\nversion: 1.0.0\ndeclares:\n  permissions: read-only\n\
         contents:\n  workflows: [\"../../../outside.yaml\"]\n",
    )
    .await;
    write(
        &repo.join(".yunta/outside.yaml"),
        "name: outside\nnodes:\n  - { id: x, kind: bash, run: \"true\" }\n",
    )
    .await;
    let parent = workflow(
        "name: parent\nnodes:\n  - { id: call, kind: workflow, use: pub/outside }\n",
        repo,
    );
    let refused = freeze(repo, &parent, &ConfigLayer::default(), None).await;
    assert!(
        matches!(
            &refused,
            Err(FrozenError::Escapes { pack, path })
                if pack == "pub/tools" && path == "../../../outside.yaml"
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_frozen_skill_that_changed_on_disk_is_refused_before_it_mounts() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let run = root.path().join("run");
    write(&repo.join(".yunta/skills/notes/SKILL.md"), "Take notes.\n").await;
    write(
        &repo.join(".yunta/agents/reviewer.md"),
        "---\nname: reviewer\ndescription: d\n---\nWork as reviewer.\n",
    )
    .await;
    let parent = workflow(
        "name: frozen\nnodes:\n  - { id: audit, kind: prompt, runner: executor, agent: reviewer, skills: [notes], prompt: \"x\" }\n",
        &repo,
    );
    let (frozen, blobs) = freeze(&repo, &parent, &ConfigLayer::default(), None)
        .await
        .unwrap();
    store::write_at_birth(&run, &blobs, Some(&frozen))
        .await
        .unwrap();

    let source = Source::Frozen {
        frozen: &frozen,
        run_dir: &run,
    };
    let agent = source.agent(&"reviewer".parse().unwrap()).await.unwrap();
    assert_eq!(agent.instructions.trim(), "Work as reviewer.");
    let skill = "notes".parse().unwrap();
    let dir = source.skill_dir(&skill).await.unwrap();
    assert_eq!(
        tokio::fs::read_to_string(dir.join("SKILL.md"))
            .await
            .unwrap(),
        "Take notes.\n"
    );

    let copy = dir.join("SKILL.md");
    tokio::fs::remove_file(&copy).await.unwrap();
    tokio::fs::write(&copy, "Take notes badly.\n")
        .await
        .unwrap();
    assert!(matches!(
        source.skill_dir(&skill).await,
        Err(FrozenError::Mismatch { .. })
    ));
}

/// An org knowledge file reads under the same pack and the same path within
/// its declared root whether the run froze it or reads the checkout now.
#[tokio::test]
async fn an_org_knowledge_file_is_named_the_same_frozen_and_read_from_the_checkout() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let run = root.path().join("run");
    write(
        &repo.join(".yunta/packs/acme/review/pack.yaml"),
        "name: review\npublisher: acme\nversion: 1.0.0\ndeclares:\n  permissions: read-only\n\
         contents:\n  knowledge: [knowledge/]\n",
    )
    .await;
    write(
        &repo.join(".yunta/packs/acme/review/knowledge/notes/0001.md"),
        "Decided.\n",
    )
    .await;
    let parent = workflow(
        "name: knowing\nnodes:\n  - { id: think, kind: prompt, runner: executor, prompt: \"x\", context: [{ knowledge: {} }] }\n",
        &repo,
    );
    let config = ConfigLayer::default();
    let (frozen, blobs) = freeze(&repo, &parent, &config, None).await.unwrap();
    store::write_at_birth(&run, &blobs, Some(&frozen))
        .await
        .unwrap();

    let named = |files: Vec<KnowledgeFile>| -> Vec<(Option<String>, String)> {
        files
            .into_iter()
            .map(|file| (file.pack, file.relative))
            .collect()
    };
    let frozen_source = Source::Frozen {
        frozen: &frozen,
        run_dir: &run,
    };
    let checkout = Source::Checkout {
        worktree: &repo,
        config: &config,
        ambient: None,
    };
    let frozen_names = named(frozen_source.knowledge(KnowledgeLayer::Org).await.unwrap());
    let checkout_names = named(checkout.knowledge(KnowledgeLayer::Org).await.unwrap());
    assert_eq!(frozen_names, checkout_names);
    assert_eq!(
        checkout_names,
        [(Some("acme/review".to_string()), "notes/0001.md".to_string())]
    );
}

/// An agent whose file has no front matter is no definition a node can read:
/// the freeze refuses the run, and the refusal names the agent.
#[tokio::test]
async fn an_agent_without_front_matter_is_refused_at_freeze_by_name() {
    let refused = freeze_reviewer("Work as reviewer.\n").await;
    assert!(
        matches!(
            &refused,
            Err(FrozenError::Unresolved(text)) if text.contains("agent `reviewer`")
        ),
        "{refused:?}"
    );
}

/// An agent whose file is not UTF-8 text is refused the same way.
#[tokio::test]
async fn an_agent_file_that_is_not_text_is_refused_at_freeze_by_name() {
    let refused = freeze_reviewer(b"\xff\xfe\x00").await;
    assert!(
        matches!(
            &refused,
            Err(FrozenError::Unresolved(text)) if text.contains("agent `reviewer`")
        ),
        "{refused:?}"
    );
}
