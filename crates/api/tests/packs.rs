//! Two pack commands at once both stand in `.yunta/yunta.lock`: a pack
//! command holds the repository's packs from its read of the lock to its
//! write, so another one waits rather than writing over an entry it never
//! read.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use yunta_api::{Error, Remark, Stop, Workspace};
use yunta_core::disk::SystemDisk;
use yunta_core::pace::Pace;
use yunta_core::persisted::PersistedDoc;
use yunta_core::refusal::{PackRefusal, SourceDirectoryFault};
use yunta_core::{
    sha256_hex, Env, PackLock, PackLockEntry, PackRef, ProcessSecrets, SystemIdSource,
};
use yunta_engine::lock::{self, Acquired, Contention, SystemProbe};
use yunta_render::refusal::Refused;
use yunta_testkit::{git, init_repo};
use yunta_testkit_core::FixedClock;

#[path = "packs/reaching_past.rs"]
mod reaching_past;

/// How long the test gives the install to write before it looks: many polls
/// of a waiting pack command, and far longer than a local clone, vendor and
/// lock write take, so an install the hold does not stop has written by
/// then.
const WRITE_WINDOW: Duration = Duration::from_millis(800);

/// A pack command holds the repository's packs from its read of the lock to
/// its write: an install that finds them held waits, writing nothing, and
/// once they are let go it reads the lock again, so the entry another
/// command wrote meanwhile stands beside its own.
#[tokio::test]
async fn an_install_waits_for_the_packs_held_and_keeps_the_entry_written_meanwhile() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    pack_repo(&upstream);
    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    init_repo(&repo);
    let workspace = workspace(&repo, root.path());
    let stop = Stop::new();
    let packs = workspace.packs();
    let fetched = packs
        .fetch(upstream.to_str().unwrap(), false, &stop)
        .await
        .unwrap();

    let held = hold(&repo).await;
    let quiet = |_: Remark| {};
    let install = fetched.install(&quiet, &stop);
    let meanwhile = async {
        Pace::every(WRITE_WINDOW).beat().await;
        assert!(
            !repo.join(".yunta/yunta.lock").exists(),
            "the install wrote the lock while the packs were held"
        );
        assert!(
            !repo.join(".yunta/packs/acme/review-pack").exists(),
            "the install vendored its pack while the packs were held"
        );
        write_lock(&repo, &other_entry());
        drop(held);
    };
    let (installed, ()) = tokio::join!(install, meanwhile);
    installed.unwrap();

    let lock = read_lock(&repo);
    let named: Vec<String> = lock.packs.keys().map(ToString::to_string).collect();
    assert_eq!(named, ["acme/review-pack", "other/notes-pack"]);
}

/// What decides an install is judged again once the packs are held: of two
/// adds of one pack fetched before either installed, the second finds the
/// first's and is refused, leaving the installed tree and the lock as the
/// first wrote them.
#[tokio::test]
async fn the_second_of_two_adds_of_one_pack_is_refused_once_the_first_installed() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    pack_repo(&upstream);
    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    init_repo(&repo);
    let workspace = workspace(&repo, root.path());
    let stop = Stop::new();
    let packs = workspace.packs();
    let source = upstream.to_str().unwrap();
    let first = packs.fetch(source, false, &stop).await.unwrap();
    let second = packs.fetch(source, false, &stop).await.unwrap();
    let quiet = |_: Remark| {};
    first.install(&quiet, &stop).await.unwrap();
    let locked = read_lock(&repo);

    let refused = second.install(&quiet, &stop).await.err();

    assert!(
        matches!(
            refused,
            Some(Error::Refused(Refused::Pack(ref refusal)))
                if matches!(**refusal, PackRefusal::AlreadyInstalled { .. })
        ),
        "{refused:?}"
    );
    assert_eq!(read_lock(&repo), locked);
}

/// An update fetched before another command removed its pack finds the pack
/// gone once the packs are held, and is refused rather than writing it back
/// into the lock.
#[tokio::test]
async fn an_update_that_finds_its_pack_removed_is_refused_and_writes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    pack_repo(&upstream);
    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    init_repo(&repo);
    let workspace = workspace(&repo, root.path());
    let stop = Stop::new();
    let packs = workspace.packs();
    let quiet = |_: Remark| {};
    let fetched = packs
        .fetch(upstream.to_str().unwrap(), false, &stop)
        .await
        .unwrap();
    let pack = fetched.install(&quiet, &stop).await.unwrap().pack;
    let update = packs
        .fetch_update(&pack, "HEAD", false, &stop)
        .await
        .unwrap();
    packs.remove(&pack, &quiet, &stop).await.unwrap();

    let refused = update.install(&quiet, &stop).await.err();

    assert!(
        matches!(
            refused,
            Some(Error::Refused(Refused::Pack(ref refusal)))
                if matches!(**refusal, PackRefusal::NotInstalled { .. })
        ),
        "{refused:?}"
    );
    assert!(read_lock(&repo).packs.is_empty());
    assert!(!repo.join(".yunta/packs/acme/review-pack").exists());
}

/// A local source written relative is the workspace's, not the process's: a
/// pack named `../upstream` beside the repository installs whatever
/// directory the process happens to run in.
#[tokio::test]
async fn a_relative_source_is_read_from_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    pack_repo(&root.path().join("upstream"));
    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    init_repo(&repo);
    let workspace = workspace(&repo, root.path());
    let stop = Stop::new();
    let quiet = |_: Remark| {};

    let fetched = workspace.packs().fetch("../upstream", false, &stop).await;
    let installed = fetched.unwrap().install(&quiet, &stop).await.unwrap();

    assert_eq!(installed.pack.to_string(), "acme/review-pack");
    assert!(repo
        .join(".yunta/packs/acme/review-pack/pack.yaml")
        .exists());
}

/// A pack standing at a directory of a larger repository, named
/// `<repository>//<directory>`, installs as the same pack at a repository's
/// root does: the same manifest, the same audit, the same vendored tree and
/// content hash. Nothing of the repository outside that directory reaches the
/// project, and the lock records the source with its directory.
#[tokio::test]
async fn a_pack_at_a_directory_of_its_repository_installs_as_one_at_a_root_does() {
    let root = tempfile::tempdir().unwrap();
    let alone = root.path().join("alone");
    pack_repo(&alone);
    let monorepo = root.path().join("monorepo");
    monorepo_holding_the_pack(&monorepo, &root.path().join("elsewhere"));
    let stop = Stop::new();
    let quiet = |_: Remark| {};

    let from_root = project(&root.path().join("from-root"), root.path());
    let fetched = from_root
        .packs()
        .fetch(alone.to_str().unwrap(), false, &stop)
        .await
        .unwrap();
    let audit_at_root = format!("{:#?}", fetched.audit());
    let at_root = fetched.install(&quiet, &stop).await.unwrap();

    let from_directory = project(&root.path().join("from-directory"), root.path());
    let source = format!("{}//packs/demo", monorepo.display());
    let fetched = from_directory
        .packs()
        .fetch(&source, false, &stop)
        .await
        .unwrap();
    let audit_at_directory = format!("{:#?}", fetched.audit());
    let at_directory = fetched.install(&quiet, &stop).await.unwrap();

    assert_eq!(at_directory.pack, at_root.pack);
    assert_eq!(at_directory.manifest, at_root.manifest);
    assert_eq!(audit_at_directory, audit_at_root);
    assert_eq!(files_under(&at_directory.dir), files_under(&at_root.dir));
    assert_eq!(
        files_under(&at_directory.dir),
        [".gitkeep", "pack.yaml", "workflows/review.yaml"]
    );
    let pack = at_root.pack;
    let locked_at_root = read_lock(&from_root.dir).packs[&pack].clone();
    let locked_at_directory = read_lock(&from_directory.dir).packs[&pack].clone();
    assert_eq!(
        locked_at_directory.content_hash,
        locked_at_root.content_hash
    );
    assert_eq!(locked_at_directory.source, source);
}

/// A directory written to climb out with `..` is refused before anything is
/// cloned: the repository it names does not exist, so a clone would fail.
#[tokio::test]
async fn a_directory_that_climbs_out_of_its_repository_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let workspace = project(&root.path().join("repo"), root.path());
    let source = format!("{}//packs/../../etc", root.path().join("absent").display());

    let refused = workspace.packs().fetch(&source, false, &Stop::new()).await;

    assert_eq!(
        source_directory_refusal(refused),
        (
            source,
            "packs/../../etc".to_string(),
            SourceDirectoryFault::Climbs
        )
    );
}

/// A directory written from a filesystem's root is refused before anything is
/// cloned.
#[tokio::test]
async fn an_absolute_directory_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let workspace = project(&root.path().join("repo"), root.path());
    let source = format!("{}///etc", root.path().join("absent").display());

    let refused = workspace.packs().fetch(&source, false, &Stop::new()).await;

    assert_eq!(
        source_directory_refusal(refused),
        (source, "/etc".to_string(), SourceDirectoryFault::Absolute)
    );
}

/// A directory the repository reaches through a symbolic link is refused,
/// though the link leads to a pack that would install: the pack is fetched
/// from the repository's own directories only, and nothing is vendored.
#[tokio::test]
async fn a_directory_reached_through_a_symbolic_link_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let elsewhere = root.path().join("elsewhere");
    let monorepo = root.path().join("monorepo");
    monorepo_holding_the_pack(&monorepo, &elsewhere);
    let repo = root.path().join("repo");
    let workspace = project(&repo, root.path());
    let source = format!("{}//packs/linked", monorepo.display());

    let refused = workspace.packs().fetch(&source, false, &Stop::new()).await;

    let link = "packs/linked".to_string();
    assert_eq!(
        source_directory_refusal(refused),
        (
            source,
            "packs/linked".to_string(),
            SourceDirectoryFault::Linked { link }
        )
    );
    assert!(!repo.join(".yunta/packs").exists());
}

/// A directory the repository does not hold is refused, naming it.
#[tokio::test]
async fn a_directory_its_repository_does_not_hold_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let monorepo = root.path().join("monorepo");
    monorepo_holding_the_pack(&monorepo, &root.path().join("elsewhere"));
    let workspace = project(&root.path().join("repo"), root.path());
    let source = format!("{}//packs/absent", monorepo.display());

    let refused = workspace.packs().fetch(&source, false, &Stop::new()).await;

    assert_eq!(
        source_directory_refusal(refused),
        (
            source,
            "packs/absent".to_string(),
            SourceDirectoryFault::Missing
        )
    );
}

/// The source, the directory and the fault of a fetch refused for the
/// directory its source names, or a panic naming what it was instead.
fn source_directory_refusal(
    fetched: Result<yunta_api::FetchedPack, Error>,
) -> (String, String, SourceDirectoryFault) {
    match fetched {
        Err(Error::Refused(Refused::Pack(refusal))) => match *refusal {
            PackRefusal::SourceDirectory {
                named,
                directory,
                why,
            } => (named, directory, why),
            other => panic!("refused for another reason: {other:?}"),
        },
        Err(other) => panic!("not refused: {other:?}"),
        Ok(_) => panic!("fetched"),
    }
}

/// A repository at `repo` with `.yunta/`, and a workspace working in it.
fn project(repo: &Path, root: &Path) -> Workspace {
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    init_repo(repo);
    workspace(repo, root)
}

/// A repository at `dir` holding, at `packs/demo`, the tree `pack_repo`
/// commits at a repository's root (the `.gitkeep` it begins with included),
/// beside files of its own outside it: a README at its root, another
/// directory under `packs/`, and `packs/linked`, a symbolic link to
/// `elsewhere`, a directory outside the repository holding the same pack.
fn monorepo_holding_the_pack(dir: &Path, elsewhere: &Path) {
    let pack = dir.join("packs/demo");
    std::fs::create_dir_all(&pack).unwrap();
    init_repo(dir);
    write_pack(&pack);
    std::fs::write(pack.join(".gitkeep"), "").unwrap();
    std::fs::write(dir.join("README.md"), "a repository of many things\n").unwrap();
    std::fs::create_dir_all(dir.join("packs/other")).unwrap();
    std::fs::write(dir.join("packs/other/notes.md"), "not the pack\n").unwrap();
    write_pack(elsewhere);
    std::os::unix::fs::symlink(elsewhere, dir.join("packs/linked")).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "a pack among other things"]);
}

/// Every file under `dir`, relative to it, sorted.
fn files_under(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            match path.is_dir() {
                true => walk(root, &path, out),
                false => out.push(path.strip_prefix(root).unwrap().display().to_string()),
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort();
    files
}

/// The packs held by the test, as another pack command would hold them;
/// dropped, they are let go.
struct Held {
    path: std::path::PathBuf,
    owner: lock::LockOwner,
}

impl Drop for Held {
    fn drop(&mut self) {
        lock::release(&self.path, &self.owner).unwrap();
    }
}

/// Holds `repo`'s packs.
async fn hold(repo: &Path) -> Held {
    let path = repo.join(".yunta/yunta.lock.held");
    let acquired = lock::acquire(
        &path,
        Contention::Refuse,
        &SystemProbe,
        &FixedClock,
        &CancellationToken::new(),
        &|_| false,
    )
    .await
    .unwrap();
    let Acquired::Fresh(owner) = acquired else {
        panic!("nothing held the packs before the test");
    };
    Held { path, owner }
}

/// A workspace working in `repo`, its state under `root`, finding git on the
/// test's own `PATH`.
fn workspace(repo: &Path, root: &Path) -> Workspace {
    let home = root.join("home");
    // Git is found on the test's own `PATH`; nothing else of its process
    // environment reaches the workspace.
    let env = Env {
        home: Some(home.clone()),
        yunta_home: Some(home),
        org_config: Some(root.join("org.yaml")),
        path: Env::of_process(&[]).path,
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

/// A pack repository at `dir`, `acme/review-pack`, with one workflow and no
/// executors, committed.
fn pack_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    init_repo(dir);
    write_pack(dir);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "v1"]);
}

/// The files of `acme/review-pack` written at `dir`: its manifest and its one
/// workflow.
fn write_pack(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         description: v1\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        "name: review\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
}

/// The entry another pack command writes while the test holds the packs.
fn other_entry() -> (PackRef, PackLockEntry) {
    let pack: PackRef = "other/notes-pack".parse().unwrap();
    let entry = PackLockEntry {
        publisher: pack.publisher().clone(),
        name: pack.name().clone(),
        source: "https://example.com/other/notes-pack".to_string(),
        r#ref: "main".to_string(),
        commit: "0123456789abcdef0123456789abcdef01234567".parse().unwrap(),
        content_hash: sha256_hex(b"notes"),
    };
    (pack, entry)
}

/// Writes `repo`'s lock holding `entry` alone, as a command that read it
/// empty would.
fn write_lock(repo: &Path, (pack, entry): &(PackRef, PackLockEntry)) {
    let mut lock = PackLock::default();
    lock.packs.insert(pack.clone(), entry.clone());
    let yaml = PersistedDoc::of(lock).write().unwrap();
    std::fs::write(repo.join(".yunta/yunta.lock"), yaml).unwrap();
}

/// `repo`'s lock as it reads now.
fn read_lock(repo: &Path) -> PackLock {
    let bytes = std::fs::read(repo.join(".yunta/yunta.lock")).unwrap();
    PersistedDoc::<PackLock>::read(&bytes).unwrap().doc
}
