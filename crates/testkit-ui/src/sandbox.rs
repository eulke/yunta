//! The world a test of the application works in: a workspace in a directory of its own,
//! whose state and org layer it keeps beside it, which reads nothing of the machine's and
//! finds only the CLIs a test stands in for on its `PATH`, its git pointed where a test
//! points it, its disk read as the design's machine's where a page says what it has free;
//! packs to install from a local repository; and the instant every test stands at.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_api::{Remark, Stop, Workspace};
use yunta_core::disk::{Disk, Space};
use yunta_core::process::SearchPath;
use yunta_core::{Env, ProcessSecrets, SystemIdSource};
use yunta_testkit_core::FixedClock;

/// A workspace working in `dir`, its state kept in `root` and its org layer there too,
/// and a `PATH` git stands on, which holds no adapter's CLI.
pub fn workspace(dir: &Path, root: &Path) -> Workspace {
    workspace_on(dir, root, Vec::new())
}

/// The directories git stands in on every machine a test runs on.
const GITS_DIRECTORIES: [&str; 2] = ["/usr/bin", "/bin"];

/// A workspace as [`workspace`] makes it, whose `PATH` holds `first` before git's
/// directories: where a test puts the CLIs it stands in for.
pub fn workspace_on(dir: &Path, root: &Path, first: Vec<PathBuf>) -> Workspace {
    let mut dirs = first;
    dirs.extend(GITS_DIRECTORIES.map(PathBuf::from));
    workspace_searching(dir, root, dirs)
}

/// A workspace working in `dir`, its state kept in `root` and its org layer there too, whose
/// `PATH` holds `dirs` alone.
fn workspace_searching(dir: &Path, root: &Path, dirs: Vec<PathBuf>) -> Workspace {
    let env = Env {
        yunta_home: Some(root.to_path_buf()),
        org_config: Some(root.join("org.yaml")),
        path: SearchPath::new(dirs),
        ..Env::default()
    };
    Workspace {
        dir: dir.to_path_buf(),
        env,
        clock: Arc::new(FixedClock),
        ids: Arc::new(SystemIdSource),
        secrets: Arc::new(ProcessSecrets),
        forges: Arc::new(yunta_api::DeclaredForge),
        binary: PathBuf::from("yunta"),
        disk: Arc::new(DesignsDisk),
    }
}

/// What the disk of every place the kit makes has free, as a page says it: what the design's
/// machine had free, so what doctor and upkeep say of it reads the same on every machine.
const DESIGNS_FREE: u64 = 74_200_000_000;

/// How much the disk of every place the kit makes holds, as a page reads it: the design's
/// machine's, whose checkouts were given a fifth of it, 98.9 GB.
const DESIGNS_DISK: u64 = 494_500_000_000;

/// The disk every place the kit makes stands on, as a page that says what it has free reads
/// it: the design's machine's. Not what keeps a run room, which reads the machine's own.
struct DesignsDisk;

impl Disk for DesignsDisk {
    fn space(&self, _path: &Path) -> std::io::Result<Space> {
        Ok(Space {
            total: DESIGNS_DISK,
            available: DESIGNS_FREE,
        })
    }
}

/// A workspace working in `dir`, its state kept in `root`, whose `PATH` holds git alone, a
/// link to the machine's under `root`: no adapter's CLI answers there, and no C compiler
/// links the probe doctor times a new binary's first run with, a figure of the machine's own
/// that a macOS machine finds and another does not.
pub fn workspace_on_git_alone(dir: &Path, root: &Path) -> Workspace {
    let machines = SearchPath::new(GITS_DIRECTORIES.map(PathBuf::from).to_vec());
    let git = machines.find("git").expect("git on the machine");
    let alone = root.join("git-alone");
    std::fs::create_dir_all(&alone).expect("a directory for git alone");
    let link = alone.join("git");
    if !link.exists() {
        std::os::unix::fs::symlink(&git, &link).expect("a link to the machine's git");
    }
    workspace_searching(dir, root, vec![alone])
}

/// A workspace as [`workspace_on`] makes it, whose git reads `git`, the environment a
/// test points git with, for every process it starts.
pub fn workspace_reaching(
    dir: &Path,
    root: &Path,
    first: Vec<PathBuf>,
    git: Vec<(String, String)>,
) -> Workspace {
    let mut workspace = workspace_on(dir, root, first);
    workspace.env.subprocess_vars = git;
    workspace
}

/// A directory under `root` holding a `claude` that answers `--version` as Claude
/// Code does: on a workspace's `PATH`, the adapter this machine answers for. It opens
/// no session.
pub fn claude_answering(root: &Path) -> PathBuf {
    answering(&root.join("bin"), "claude", "2.1.0 (Claude Code)")
}

/// A directory under `root` holding a `codex` that answers `--version` as the Codex CLI
/// does, at the lowest version Yunta takes: beside [`claude_answering`]'s on a
/// workspace's `PATH`, a second adapter this machine answers for. It opens no session.
pub fn codex_answering(root: &Path) -> PathBuf {
    answering(&root.join("codex-bin"), "codex", "codex-cli 0.157.0")
}

/// The directory `bin`, holding a `cli` that says `version` when asked for one.
fn answering(bin: &Path, cli: &str, version: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(bin).expect("a directory for the CLIs");
    let stub = bin.join(cli);
    std::fs::write(&stub, format!("#!/bin/sh\necho '{version}'\n")).expect("a stub CLI");
    let executable = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&stub, executable).expect("a stub CLI that runs");
    bin.to_path_buf()
}

/// A pack's manifest: one workflow, nothing it executes.
const MANIFEST: &str = "name: demo\npublisher: acme\nversion: 1.0.0\ndeclares:\n  \
                        permissions: read-only\n  network: false\n  executors: []\n\
                        contents:\n  workflows: [workflows/demo-says.yaml]\n";

/// The pack's one workflow.
const DEMO_SAYS: &str = "name: demo-says\nnodes:\n  - { id: say, kind: bash, run: \"true\" }\n";

/// A repository set up for Yunta under `root`, with `acme/demo` installed from a
/// local repository beside it ([`pack_installed_from`]).
pub async fn with_a_pack_installed(root: &Path) -> PathBuf {
    let upstream = pack_installed_from(root);
    tokio::fs::create_dir_all(upstream.join("workflows"))
        .await
        .expect("the pack's directory");
    yunta_testkit::init_repo(&upstream);
    tokio::fs::write(upstream.join("pack.yaml"), MANIFEST)
        .await
        .expect("its manifest");
    tokio::fs::write(upstream.join("workflows/demo-says.yaml"), DEMO_SAYS)
        .await
        .expect("its workflow");
    yunta_testkit::git(&upstream, &["add", "."]);
    yunta_testkit::git(&upstream, &["commit", "-q", "-m", "v1"]);
    let repo = root.join("project");
    tokio::fs::create_dir_all(repo.join(".yunta"))
        .await
        .expect("its .yunta");
    yunta_testkit::init_repo(&repo);
    tokio::fs::write(
        repo.join(".yunta/config.yaml"),
        "defaults:\n  isolation: none\n",
    )
    .await
    .expect("its config");
    let source = upstream.to_str().expect("a path that reads");
    let stop = Stop::new();
    let remarks = |_: Remark| {};
    let installing = workspace(&repo, root);
    let fetched = installing
        .packs()
        .fetch(source, false, &stop)
        .await
        .expect("the pack fetched");
    fetched
        .install(&remarks, &stop)
        .await
        .expect("the pack installed");
    repo
}

/// A local repository under `root` holding the pack `acme/tools`, which ships the
/// executor `runner.py`, committed once; and the commit it stands at.
pub fn pack_with_an_executor(root: &Path) -> (PathBuf, String) {
    let upstream = root.join("tools");
    std::fs::create_dir_all(upstream.join("workflows")).expect("the pack's directory");
    yunta_testkit::init_repo(&upstream);
    let manifest = "name: tools\npublisher: acme\nversion: 1.0.0\ndeclares:\n  \
                    permissions: read-only\n  network: false\n  executors: [runner.py]\n\
                    contents:\n  workflows: [workflows/noop.yaml]\n";
    std::fs::write(upstream.join("pack.yaml"), manifest).expect("its manifest");
    let noop = "name: noop\nnodes:\n  - { id: noop, kind: bash, run: \"true\" }\n";
    std::fs::write(upstream.join("workflows/noop.yaml"), noop).expect("its workflow");
    std::fs::write(upstream.join("runner.py"), "print('runs')\n").expect("its executor");
    yunta_testkit::git(&upstream, &["add", "."]);
    yunta_testkit::git(&upstream, &["commit", "-q", "-m", "v1"]);
    let head = commit_of(&upstream);
    (upstream, head)
}

/// The local repository under `root` that [`with_a_pack_installed`] installs `acme/demo`
/// from.
pub fn pack_installed_from(root: &Path) -> PathBuf {
    root.join("upstream")
}

/// The commit `repo` stands at.
pub fn commit_of(repo: &Path) -> String {
    yunta_testkit::git_output(repo, &["rev-parse", "HEAD"])
        .trim()
        .to_string()
}

/// The instant the application stands at in a test.
pub fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000, 0).expect("an instant")
}
