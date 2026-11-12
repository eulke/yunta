//! Two pack commands at once both stand in `.yunta/yunta.lock`: a pack
//! command holds the repository's packs from its read of the lock to its
//! write, so another one waits rather than writing over an entry it never
//! read.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use yunta_api::{Error, Remark, Stop, Workspace};
use yunta_core::pace::Pace;
use yunta_core::persisted::PersistedDoc;
use yunta_core::refusal::PackRefusal;
use yunta_core::{
    sha256_hex, Env, PackLock, PackLockEntry, PackRef, ProcessSecrets, SystemIdSource,
};
use yunta_engine::lock::{self, Acquired, Contention, SystemProbe};
use yunta_render::refusal::Refused;
use yunta_testkit::{git, init_repo};
use yunta_testkit_core::FixedClock;

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
        binary: "yunta".into(),
    }
}

/// A pack repository at `dir`, `acme/review-pack`, with one workflow and no
/// executors, committed.
fn pack_repo(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    init_repo(dir);
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
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "v1"]);
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
