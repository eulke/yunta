//! The world a test of the application works in: a workspace in a directory of its own,
//! whose state and org layer it keeps beside it, which reads nothing of the machine's and
//! finds only the CLIs a test stands in for on its `PATH`; packs to install from a local
//! repository; and the instant every test stands at.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use yunta_api::{Remark, Stop, Workspace};
use yunta_core::process::SearchPath;
use yunta_core::{Env, ProcessSecrets, SystemIdSource};
use yunta_testkit_core::FixedClock;

/// A workspace working in `dir`, its state kept in `root` and its org layer there too,
/// and a `PATH` git stands on, which holds no adapter's CLI.
pub fn workspace(dir: &Path, root: &Path) -> Workspace {
    workspace_on(dir, root, Vec::new())
}

/// A workspace as [`workspace`] makes it, whose `PATH` holds `first` before git's
/// directories: where a test puts the CLIs it stands in for.
pub fn workspace_on(dir: &Path, root: &Path, first: Vec<PathBuf>) -> Workspace {
    let mut dirs = first;
    dirs.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
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
        binary: PathBuf::from("yunta"),
    }
}

/// A directory under `root` holding a `claude` that answers `--version` as Claude
/// Code does: on a workspace's `PATH`, the adapter this machine answers for. It opens
/// no session.
pub fn claude_answering(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).expect("a directory for the CLIs");
    let claude = bin.join("claude");
    std::fs::write(&claude, "#!/bin/sh\necho '2.1.0 (Claude Code)'\n").expect("a stub CLI");
    let executable = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&claude, executable).expect("a stub CLI that runs");
    bin
}

/// A pack's manifest: one workflow, nothing it executes.
const MANIFEST: &str = "name: demo\npublisher: acme\nversion: 1.0.0\ndeclares:\n  \
                        permissions: read-only\n  network: false\n  executors: []\n\
                        contents:\n  workflows: [workflows/demo-says.yaml]\n";

/// The pack's one workflow.
const DEMO_SAYS: &str = "name: demo-says\nnodes:\n  - { id: say, kind: bash, run: \"true\" }\n";

/// A repository set up for Yunta under `root`, with `acme/demo` installed from a
/// local repository beside it.
pub async fn with_a_pack_installed(root: &Path) -> PathBuf {
    let upstream = root.join("upstream");
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
