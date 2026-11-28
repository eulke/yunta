//! The pack a setup installs, and the runner roles a setup proposes with it: the Yunta
//! repository's reference pack at this binary's version tag, fetched here from the
//! test kit's stand-in for that repository, the tag alone; a draft proposing with it
//! fetched proposes what `init` proposes once that pack is installed; and a setup writes
//! the default runner of the proposal it was answered from.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_api::{ProposedRunners, SetupPack, Stop, Workspace};
use yunta_core::disk::SystemDisk;
use yunta_core::process::SearchPath;
use yunta_core::{Env, PackRef, ProcessSecrets, RunnerName, SystemIdSource};
use yunta_testkit::reference_pack::{ReferencePack, FRAGUA, VERSION_TAG, YUNTA_REPOSITORY};
use yunta_testkit::{git_output, init_repo, write};
use yunta_testkit_core::FixedClock;

#[test]
fn the_setup_pack_names_this_binary_s_version() {
    let pack = SetupPack::of_this_binary();
    let fragua: PackRef = "yunta/fragua".parse().expect("a pack");
    assert_eq!(pack.pack(), &fragua);
    let at_the_version = format!("{YUNTA_REPOSITORY}//{FRAGUA}@{VERSION_TAG}");
    assert_eq!(pack.source(), at_the_version);
    assert!(
        pack.source()
            .ends_with(&format!("@v{}", env!("CARGO_PKG_VERSION"))),
        "{}",
        pack.source()
    );
}

/// A repository with no workflow, its draft proposing with the reference pack fetched,
/// proposes fragua's five roles, in the order `init` proposes them once the pack is
/// installed.
#[tokio::test]
async fn a_repository_with_no_workflow_proposes_the_fetched_pack_s_roles() {
    let root = tempfile::tempdir().expect("a temporary root");
    let reference = ReferencePack::made(&root.path().join("upstream/yunta"));
    let proposing = never_set_up(root.path(), "proposing");
    let installed = never_set_up(root.path(), "installed");

    let proposed = proposed_with_the_pack_fetched(&proposing, root.path(), &reference).await;
    let after_install = proposed_after_the_install(&installed, root.path(), &reference).await;

    assert_eq!(proposed, after_install);
    let roles: Vec<&str> = proposed.roles().iter().map(RunnerName::as_str).collect();
    assert_eq!(
        roles,
        [
            "planner",
            "executor",
            "reviewer",
            "reviewer-alt",
            "mechanical"
        ]
    );
    assert!(
        !proposing.join(".yunta").exists(),
        "proposing with a fetched pack writes nothing into the repository"
    );
}

/// A repository whose own workflow names roles proposes them first, then the fetched
/// pack's it does not name, as `init` does once the pack is installed.
#[tokio::test]
async fn the_repository_s_own_roles_come_before_the_pack_s() {
    let root = tempfile::tempdir().expect("a temporary root");
    let reference = ReferencePack::made(&root.path().join("upstream/yunta"));
    let proposing = never_set_up(root.path(), "proposing");
    let installed = never_set_up(root.path(), "installed");
    for repo in [&proposing, &installed] {
        write(&repo.join(".yunta/workflows/docs.yaml"), DOCS);
    }

    let proposed = proposed_with_the_pack_fetched(&proposing, root.path(), &reference).await;
    let after_install = proposed_after_the_install(&installed, root.path(), &reference).await;

    assert_eq!(proposed, after_install);
    let roles: Vec<&str> = proposed.roles().iter().map(RunnerName::as_str).collect();
    assert_eq!(
        roles,
        [
            "writer",
            "reviewer",
            "planner",
            "executor",
            "reviewer-alt",
            "mechanical"
        ]
    );
}

/// The setup's pack, named by its tag, is cloned at that tag alone: of a repository whose
/// tag stands on a commit after another, the clone holds one commit.
#[tokio::test]
async fn the_setup_s_pack_is_cloned_at_its_tag_alone() {
    let root = tempfile::tempdir().expect("a temporary root");
    let reference = ReferencePack::made_with_history(&root.path().join("upstream/yunta"));
    let repo = never_set_up(root.path(), "fetching");
    let reached = reference.reached_through_its_url();
    let workspace = workspace_reaching(&repo, root.path(), reached);

    let fetched = SetupPack::of_this_binary()
        .fetched(&workspace, &Stop::new())
        .await
        .expect("the setup's pack, fetched")
        .expect("a repository that vendors no pack fetches it");

    let commits = git_output(fetched.checkout(), &["rev-list", "--count", "HEAD"]);
    assert_eq!(commits, "1", "the tag's commit alone");
    let tagged = git_output(reference.repository(), &["rev-parse", VERSION_TAG]);
    let cloned = git_output(fetched.checkout(), &["rev-parse", "HEAD"]);
    assert_eq!(cloned, tagged, "the commit the tag names");
}

/// A setup writes the default runner of the proposal it was answered from: a workflow
/// added to the repository after the answer, whose node names a role of its own and
/// another names none, which a proposal worked out again would make the default, changes
/// nothing of it.
#[tokio::test]
async fn a_workflow_added_after_the_answer_leaves_the_default_written() {
    let root = tempfile::tempdir().expect("a temporary root");
    let reference = ReferencePack::made(&root.path().join("upstream/yunta"));
    let repo = never_set_up(root.path(), "answered");
    let workspace = workspace(&repo, root.path(), &reference);
    let stop = Stop::new();
    let mut detected = workspace
        .detect_project(false, &stop)
        .await
        .expect("what setting it up finds");
    let fetched = workspace
        .packs()
        .fetch(SetupPack::of_this_binary().source(), false, &stop)
        .await
        .expect("the setup's pack, fetched");
    detected.proposing(&fetched);
    let answered_from = detected.draft.proposed.default_runner().cloned();
    let setup = detected.draft.answered("", "", Vec::new());

    write(&repo.join(".yunta/workflows/late.yaml"), LATE);
    detected.proposing(&fetched);
    assert_eq!(
        detected
            .draft
            .proposed
            .default_runner()
            .map(RunnerName::as_str),
        Some("late"),
        "worked out again, the proposal names the late workflow's role"
    );
    detected.write(setup).await.expect("the setup, written");

    let config = std::fs::read_to_string(repo.join(".yunta/config.yaml")).expect("the config");
    let written = default_written(&config);
    assert_eq!(
        written.as_deref(),
        answered_from.as_ref().map(RunnerName::as_str),
        "the default of the proposal answered:\n{config}"
    );
}

/// A workflow added late, whose first node names a role no other workflow names and whose
/// second names none.
const LATE: &str = "\
name: late
nodes:
  - id: first
    kind: prompt
    runner: late
    prompt: do the first thing
  - id: second
    kind: prompt
    depends_on: [first]
    prompt: do the second thing
";

/// The default runner `config` writes, commented or not: the role on the line after its
/// `defaults:`.
fn default_written(config: &str) -> Option<String> {
    let mut lines = config
        .lines()
        .map(|line| line.trim_start_matches('#').trim());
    lines.find(|line| *line == "defaults:")?;
    let runner = lines.next()?.strip_prefix("runner:")?;
    Some(runner.trim().to_string())
}

/// A workflow of the repository's own whose nodes name a writer, then a reviewer, a
/// role fragua names too.
const DOCS: &str = "\
name: docs
nodes:
  - id: draft
    kind: prompt
    runner: writer
    prompt: write the docs
  - id: review
    kind: prompt
    depends_on: [draft]
    runner: reviewer
    prompt: review the docs
";

/// The roles `repo`'s draft proposes once the setup's pack, fetched through git
/// pointed at `reference`, is proposed with.
async fn proposed_with_the_pack_fetched(
    repo: &Path,
    root: &Path,
    reference: &ReferencePack,
) -> ProposedRunners {
    let workspace = workspace(repo, root, reference);
    let stop = Stop::new();
    let mut detected = workspace
        .detect_project(false, &stop)
        .await
        .expect("what setting it up finds");
    let fetched = workspace
        .packs()
        .fetch(SetupPack::of_this_binary().source(), false, &stop)
        .await
        .expect("the setup's pack, fetched");
    detected.proposing(&fetched);
    detected.draft.proposed
}

/// The roles `init` proposes in `repo` once the setup's pack, fetched through git
/// pointed at `reference`, is installed there, as `yunta pack add` installs it.
async fn proposed_after_the_install(
    repo: &Path,
    root: &Path,
    reference: &ReferencePack,
) -> ProposedRunners {
    let workspace = workspace(repo, root, reference);
    let stop = Stop::new();
    let quiet = |_: yunta_api::Remark| {};
    workspace
        .packs()
        .fetch(SetupPack::of_this_binary().source(), true, &stop)
        .await
        .expect("the setup's pack, fetched")
        .install(&quiet, &stop)
        .await
        .expect("the setup's pack, installed");
    let detected = workspace
        .detect_project(false, &stop)
        .await
        .expect("what setting it up finds");
    detected.draft.proposed
}

/// A git repository named `name` under `root`, never set up.
fn never_set_up(root: &Path, name: &str) -> PathBuf {
    let repo = root.join(name);
    std::fs::create_dir_all(&repo).expect("a repository's directory");
    init_repo(&repo);
    repo
}

/// A workspace working in `repo`, its state under `root`, whose git reads the Yunta
/// repository's URL as `reference` and finds no agent CLI on its `PATH`.
fn workspace(repo: &Path, root: &Path, reference: &ReferencePack) -> Workspace {
    workspace_reaching(repo, root, reference.reached())
}

/// A workspace working in `repo`, its state under `root`, whose git takes `reached`, the
/// variables that point it at the Yunta repository, and finds no agent CLI on its `PATH`.
fn workspace_reaching(repo: &Path, root: &Path, reached: Vec<(String, String)>) -> Workspace {
    let env = Env {
        yunta_home: Some(root.join("home")),
        org_config: Some(root.join("org.yaml")),
        path: SearchPath::new(vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")]),
        subprocess_vars: reached,
        ..Env::default()
    };
    Workspace {
        dir: repo.to_path_buf(),
        env,
        clock: Arc::new(FixedClock),
        ids: Arc::new(SystemIdSource),
        secrets: Arc::new(ProcessSecrets),
        forges: Arc::new(yunta_api::DeclaredForge),
        binary: "yunta".into(),
        disk: Arc::new(SystemDisk),
    }
}
