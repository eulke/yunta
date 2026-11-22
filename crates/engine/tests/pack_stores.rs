//! A project reads packs from its own store, then the user's under the state
//! root. The user's answers only what the project's leaves unanswered, a user
//! pack the project also installs is shadowed whole, a birth with no
//! environment reads no user pack, and a pack is held to its own manifest and
//! its own store's lock wherever it is installed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use yunta_core::check::CheckError;
use yunta_core::events::NodeState;
use yunta_core::{ConfigLayer, ContentHash, Env, PackScope, Workflow};
use yunta_engine::{
    build_manifest, check_workflow_refs, origin_of, packs_for_publisher, resolve_workflow,
    CatalogError, FrozenRun, ManifestError, RunTerminal, WorkflowOrigin,
};
use yunta_testkit::{init_repo, write, Bench, Owner};

mod common;
use common::{context_sources, context_workflow};

const LEAF: &str = "name: leaf\nnodes:\n  - { id: work, kind: bash, run: \"true\" }\n";

/// A session at the engine's default `edit` permissions: over a pack that
/// declares `read-only`.
const EDITS: &str = "name: review\nnodes:\n  - { id: draft, kind: prompt, prompt: hi }\n";

/// A repository workflow that composes `acme/review`.
const COMPOSES: &str = "name: parent\nnodes:\n  - { id: sub, kind: workflow, use: acme/review }\n";

/// A session that runs as the agent `acme/auditor`.
const ASKS: &str = "name: ask\nnodes:\n  - { id: audit, kind: prompt, runner: executor, agent: acme/auditor, prompt: \"x\" }\n";

/// A lock that pins `acme/review-pack` to one commit.
const LOCK: &str = "packs:\n  acme/review-pack:\n    publisher: acme\n    name: review-pack\n    \
    source: https://example.invalid/acme/review-pack\n    ref: v1.2.3\n    \
    commit: abcdef0123456789abcdef0123456789abcdef01\n    \
    content_hash: 4862f447f2c7f272fa2f4aaf89dadb3b1ac09105bd5864f8d1a0c9452bb0a226\n";

/// A project, and a user state root apart from it.
struct World {
    repo: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl World {
    fn new() -> Self {
        let world = World {
            repo: tempfile::tempdir().unwrap(),
            home: tempfile::tempdir().unwrap(),
        };
        init_repo(world.repo.path());
        world
    }

    fn repo(&self) -> &Path {
        self.repo.path()
    }

    /// The environment whose state root is this world's.
    fn env(&self) -> Env {
        Env {
            yunta_home: Some(self.home.path().to_path_buf()),
            ..Env::default()
        }
    }

    /// Where the project installs `publisher/name`.
    fn project_pack(&self, publisher: &str, name: &str) -> PathBuf {
        self.repo().join(".yunta/packs").join(publisher).join(name)
    }

    /// Where the user installs `publisher/name`.
    fn user_pack(&self, publisher: &str, name: &str) -> PathBuf {
        self.home.path().join("packs").join(publisher).join(name)
    }

    fn resolve(&self, name: &str) -> Result<yunta_engine::ResolvedWorkflow, CatalogError> {
        resolve_workflow(self.repo(), Some(&self.env()), name)
    }

    /// What composition checking refuses of `text`, read from `path`.
    fn refused(&self, path: &Path, text: &str) -> Vec<CheckError> {
        let env = self.env();
        let origin = origin_of(self.repo(), Some(&env), path);
        let config = ConfigLayer::default();
        let workflow = parse(text);
        check_workflow_refs(
            &workflow,
            &config,
            self.repo(),
            Some(&env),
            &origin,
            &|_| None,
        )
        .errors
    }

    /// The run `text` is born as from `workflow_dir`, under this world's
    /// environment.
    async fn freeze(&self, text: &str, workflow_dir: &Path) -> Result<FrozenRun, ManifestError> {
        let owner = Owner::new();
        let inputs = HashMap::new();
        let config = ConfigLayer::default();
        let env = self.env();
        let workflow = parse(text);
        let repo = self.repo();
        build_manifest(
            &workflow,
            &config,
            workflow_dir,
            repo,
            &inputs,
            owner.supervision(),
            Some(&env),
        )
        .await
    }
}

fn parse(text: &str) -> Workflow {
    yunta_core::yaml::parse(text).unwrap()
}

/// Writes the pack at `dir`, named by its last two segments, with
/// `permissions` as its ceiling, `contents` as the lines under its
/// `contents:` key, and each of `files`.
fn pack(dir: &Path, permissions: &str, contents: &str, files: &[(&str, &str)]) {
    let name = dir.file_name().unwrap().to_str().unwrap();
    let publisher = dir.parent().unwrap().file_name().unwrap().to_str().unwrap();
    let manifest = format!(
        "name: {name}\npublisher: {publisher}\nversion: 1.2.3\ndeclares:\n  permissions: \
         {permissions}\ncontents:\n{contents}"
    );
    write(&dir.join("pack.yaml"), &manifest);
    for (file, text) in files {
        write(&dir.join(file), text);
    }
}

/// A `read-only` pack at `dir` declaring each of `workflows`, each a leaf.
fn workflow_pack(dir: &Path, workflows: &[&str]) {
    let contents = format!("  workflows: [{}]\n", workflows.join(", "));
    let files: Vec<(&str, &str)> = workflows.iter().map(|file| (*file, LEAF)).collect();
    pack(dir, "read-only", &contents, &files);
}

/// A pack at `dir` declaring `permissions` and `review.yaml`, a session at
/// the default permissions.
fn editing_pack(dir: &Path, permissions: &str) {
    let files = [("review.yaml", EDITS)];
    pack(dir, permissions, "  workflows: [review.yaml]\n", &files);
}

/// Whether `errors` refuse the node `draft` for going over the `declared`
/// ceiling of `acme/review-pack`.
fn over_ceiling(errors: &[CheckError], declared: &str) -> bool {
    errors.iter().any(|error| {
        matches!(
            error,
            CheckError::PackPermissionsCeilingExceeded { node, pack, declared: ceiling, .. }
                if node.as_str() == "draft" && pack == "acme/review-pack" && *ceiling == declared
        )
    })
}

#[test]
fn a_workflow_only_a_user_pack_declares_resolves_from_the_user_s_store() {
    let world = World::new();
    let installed = world.user_pack("acme", "review-pack");
    workflow_pack(&installed, &["review.yaml"]);

    let resolved = world.resolve("acme/review").unwrap();

    assert_eq!(resolved.path, installed.join("review.yaml"));
    assert_eq!(resolved.origin.pack_dir(), Some(installed));
    assert!(
        matches!(&resolved.origin, WorkflowOrigin::Pack { store, .. } if store.scope() == PackScope::User),
        "{:?}",
        resolved.origin
    );
}

#[test]
fn a_project_pack_answers_a_workflow_a_user_pack_of_another_name_also_declares() {
    let world = World::new();
    workflow_pack(
        &world.project_pack("acme", "project-pack"),
        &["review.yaml"],
    );
    workflow_pack(&world.user_pack("acme", "user-pack"), &["review.yaml"]);

    let resolved = world.resolve("acme/review").unwrap();

    let project = world.project_pack("acme", "project-pack");
    assert_eq!(resolved.path, project.join("review.yaml"));
}

#[test]
fn a_project_pack_shadows_the_user_s_pack_of_the_same_name_whole() {
    let world = World::new();
    workflow_pack(&world.project_pack("acme", "review-pack"), &["review.yaml"]);
    workflow_pack(
        &world.user_pack("acme", "review-pack"),
        &["review.yaml", "extra.yaml"],
    );

    let project = world.project_pack("acme", "review-pack");
    assert_eq!(
        world.resolve("acme/review").unwrap().path,
        project.join("review.yaml")
    );
    let shadowed = world.resolve("acme/extra");
    assert!(
        matches!(shadowed, Err(CatalogError::NotFound { .. })),
        "{shadowed:?}"
    );
}

#[test]
fn two_user_packs_declaring_one_workflow_are_ambiguous_until_a_project_pack_declares_it() {
    let world = World::new();
    workflow_pack(&world.user_pack("acme", "pack-a"), &["review.yaml"]);
    workflow_pack(&world.user_pack("acme", "pack-b"), &["review.yaml"]);
    let ambiguous = world.resolve("acme/review");
    assert!(
        matches!(ambiguous, Err(CatalogError::Ambiguous { count: 2, .. })),
        "{ambiguous:?}"
    );

    let project = world.project_pack("acme", "pack-c");
    workflow_pack(&project, &["review.yaml"]);
    assert_eq!(
        world.resolve("acme/review").unwrap().path,
        project.join("review.yaml")
    );
}

/// A run kept apart from the machine it runs on, as a case is, names none.
/// Reached by: `crates/api/src/admission.rs`
#[test]
fn a_lookup_with_no_environment_reads_no_user_pack() {
    let world = World::new();
    workflow_pack(&world.user_pack("acme", "review-pack"), &["review.yaml"]);

    let unread = resolve_workflow(world.repo(), None, "acme/review");

    assert!(
        matches!(unread, Err(CatalogError::NotFound { .. })),
        "{unread:?}"
    );
    let acme = "acme".parse().unwrap();
    assert!(packs_for_publisher(world.repo(), None, &acme)
        .installed
        .is_empty());
}

#[test]
fn a_state_root_that_is_the_project_s_own_store_reads_each_pack_once() {
    let world = World::new();
    let installed = world.project_pack("acme", "review-pack");
    workflow_pack(&installed, &["review.yaml"]);
    let env = Env {
        yunta_home: Some(world.repo().join(".yunta")),
        ..Env::default()
    };

    let resolved = resolve_workflow(world.repo(), Some(&env), "acme/review").unwrap();

    assert_eq!(resolved.path, installed.join("review.yaml"));
    let acme = "acme".parse().unwrap();
    let packs = packs_for_publisher(world.repo(), Some(&env), &acme);
    assert_eq!(packs.installed.len(), 1);
}

#[test]
fn a_user_pack_over_its_own_ceiling_is_refused_when_it_is_the_workflow_checked() {
    let world = World::new();
    let installed = world.user_pack("acme", "review-pack");
    editing_pack(&installed, "read-only");

    let errors = world.refused(&installed.join("review.yaml"), EDITS);

    assert!(over_ceiling(&errors, "read-only"), "{errors:?}");
}

#[test]
fn a_user_pack_over_its_own_ceiling_is_refused_when_a_workflow_composes_it() {
    let world = World::new();
    editing_pack(&world.user_pack("acme", "review-pack"), "read-only");
    let parent = world.repo().join(".yunta/workflows/parent.yaml");
    write(&parent, COMPOSES);

    let errors = world.refused(&parent, COMPOSES);

    assert!(over_ceiling(&errors, "read-only"), "{errors:?}");
}

#[test]
fn a_project_pack_is_held_to_its_own_ceiling_beside_a_looser_user_copy() {
    let world = World::new();
    let project = world.project_pack("acme", "review-pack");
    editing_pack(&project, "read-only");
    editing_pack(&world.user_pack("acme", "review-pack"), "full");
    let parent = world.repo().join(".yunta/workflows/parent.yaml");
    write(&parent, COMPOSES);

    let checked = world.refused(&project.join("review.yaml"), EDITS);
    let composed = world.refused(&parent, COMPOSES);

    assert!(over_ceiling(&checked, "read-only"), "{checked:?}");
    assert!(over_ceiling(&composed, "read-only"), "{composed:?}");
}

/// The definition of the agent `auditor`, with `instructions` as its body.
fn auditor(instructions: &str) -> String {
    format!("---\nname: auditor\ndescription: d\n---\n{instructions}\n")
}

/// A pack at `dir` shipping the agent `auditor` with `instructions`.
fn agent_pack(dir: &Path, instructions: &str) {
    let agent = auditor(instructions);
    let files = [("agents/auditor.md", agent.as_str())];
    pack(dir, "read-only", "  agents: [agents/auditor.md]\n", &files);
}

/// The bytes a birth holds under `hash`.
fn held(run: &FrozenRun, hash: &ContentHash) -> String {
    let (_, bytes) = run.blobs.iter().find(|(held, _)| held == hash).unwrap();
    String::from_utf8(bytes.clone()).unwrap()
}

#[tokio::test]
async fn a_birth_freezes_the_agent_a_user_pack_ships() {
    let world = World::new();
    agent_pack(
        &world.user_pack("acme", "tools"),
        "Audit as the user's pack says.",
    );

    let run = world.freeze(ASKS, world.repo()).await.unwrap();

    let frozen = run.manifest.frozen.as_ref().unwrap();
    let agent = &frozen.agents["acme/auditor"];
    assert_eq!(held(&run, agent), auditor("Audit as the user's pack says."));
}

#[tokio::test]
async fn a_project_pack_s_agent_wins_over_a_user_pack_s_of_the_same_name() {
    let world = World::new();
    agent_pack(
        &world.user_pack("acme", "tools"),
        "Audit as the user's pack says.",
    );
    agent_pack(
        &world.project_pack("acme", "project-tools"),
        "Audit as the project says.",
    );

    let run = world.freeze(ASKS, world.repo()).await.unwrap();

    let frozen = run.manifest.frozen.as_ref().unwrap();
    let agent = &frozen.agents["acme/auditor"];
    assert_eq!(held(&run, agent), auditor("Audit as the project says."));
}

#[tokio::test]
async fn a_birth_from_a_user_pack_s_workflow_records_its_version_and_the_user_s_locked_commit() {
    let world = World::new();
    let installed = world.user_pack("acme", "review-pack");
    workflow_pack(&installed, &["review.yaml"]);
    write(&world.home.path().join("yunta.lock"), LOCK);

    let run = world.freeze(LEAF, &installed).await.unwrap();

    let provenance = run
        .manifest
        .pack
        .expect("a workflow of a pack is born with it");
    assert_eq!(provenance.publisher, "acme");
    assert_eq!(provenance.name, "review-pack");
    assert_eq!(provenance.version, "1.2.3");
    let commit = provenance
        .commit
        .as_ref()
        .map(yunta_core::CommitSha::as_str);
    assert_eq!(commit, Some("abcdef0123456789abcdef0123456789abcdef01"));
}

#[tokio::test]
async fn a_prompt_file_that_leaves_a_user_pack_refuses_the_birth() {
    let world = World::new();
    let installed = world.user_pack("acme", "review-pack");
    let climbing = "name: climbing\nnodes:\n  - { id: plan, kind: prompt, runner: executor, prompt: { file: ../../../secret.txt } }\n";
    pack(
        &installed,
        "read-only",
        "  workflows: [review.yaml]\n",
        &[("review.yaml", climbing)],
    );
    write(&world.home.path().join("secret.txt"), "hunter2");

    let refused = world.freeze(climbing, &installed).await.unwrap_err();

    let said = refused.to_string();
    assert!(matches!(refused, ManifestError::Frozen(_)), "{said}");
    assert!(said.contains("acme/review-pack"), "{said}");
    assert!(said.contains("../../../secret.txt"), "{said}");
    assert!(!said.contains("hunter2"), "{said}");
}

/// A pack at `dir` shipping each of `files` as its knowledge.
fn knowledge_pack(dir: &Path, files: &[(&str, &str)]) {
    let shipped: Vec<(String, &str)> = files
        .iter()
        .map(|(file, text)| (format!("knowledge/{file}"), *text))
        .collect();
    let files: Vec<(&str, &str)> = shipped.iter().map(|(f, t)| (f.as_str(), *t)).collect();
    pack(dir, "read-only", "  knowledge: [knowledge/]\n", &files);
}

/// One session that completes only when its prompt holds `marker`.
fn answered_holding(marker: &str) -> String {
    format!(
        "dialect: claude-code\nsessions:\n  - match_prompt_contains: \"{marker}\"\n    \
         outcome: {{ type: completed, summary: ok }}\n"
    )
}

/// A bench whose user state root is `home`.
fn bench_of(home: &Path) -> Bench {
    Bench::new().with_user_state_root(home)
}

#[tokio::test]
async fn the_org_layer_holds_a_knowledge_file_a_user_pack_ships() {
    let home = tempfile::tempdir().unwrap();
    let bench = bench_of(home.path());
    let shipped = [("conventions.md", "MARKER-FROM-THE-USER-PACK\n")];
    knowledge_pack(&home.path().join("packs/acme/user-knowledge"), &shipped);

    let workflow = context_workflow("      - knowledge: { layers: [org] }\n");
    let report = bench
        .run(&workflow, &answered_holding("MARKER-FROM-THE-USER-PACK"))
        .await;

    assert_eq!(report.terminal, RunTerminal::Finished, "{:?}", report.state);
}

#[tokio::test]
async fn a_project_pack_s_knowledge_file_wins_over_a_user_pack_s_of_the_same_name() {
    let home = tempfile::tempdir().unwrap();
    let bench = bench_of(home.path());
    let project = bench.worktree.join(".yunta/packs/acme/project-knowledge");
    knowledge_pack(&project, &[("conventions.md", "MARKER-PROJECT-WINS\n")]);
    let user = home.path().join("packs/globex/user-knowledge");
    knowledge_pack(&user, &[("conventions.md", "MARKER-USER-LOSES\n")]);

    let workflow = context_workflow("      - knowledge: { layers: [org] }\n");
    let report = bench
        .run(&workflow, &answered_holding("MARKER-PROJECT-WINS"))
        .await;

    assert_eq!(report.terminal, RunTerminal::Finished, "{:?}", report.state);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let source = &context_sources(&events, "ask")[0];
    let object = bench
        .run_dir()
        .join("objects")
        .join(source.content_hash.as_str());
    let read = std::fs::read_to_string(object).unwrap();
    assert!(!read.contains("MARKER-USER-LOSES"), "{read}");
}

#[tokio::test]
async fn two_user_packs_shipping_one_knowledge_file_collide() {
    let home = tempfile::tempdir().unwrap();
    let bench = bench_of(home.path());
    let file = [("conventions.md", "from a user pack\n")];
    knowledge_pack(&home.path().join("packs/acme/pack-a"), &file);
    knowledge_pack(&home.path().join("packs/globex/pack-b"), &file);

    let workflow = context_workflow("      - knowledge: { layers: [org] }\n");
    let report = bench
        .run(&workflow, "dialect: claude-code\nsessions: []")
        .await;

    match report.state.nodes.state("ask") {
        Some(NodeState::Failed { failure, .. }) => {
            let said = yunta_render::failure::sentence(failure);
            assert!(said.contains("conventions.md"), "{said}");
            assert!(
                said.contains("acme/pack-a") && said.contains("globex/pack-b"),
                "{said}"
            );
        }
        other => panic!("expected `ask` to fail on the collision, got {other:?}"),
    }
}
