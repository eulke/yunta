//! A run remembers what its commands ran with, and says so when a later
//! wake runs them with something else.

use std::path::Path;
use std::process::{Command, Output, Stdio};

use yunta_testkit::{hermetic, init_repo, run_id_from, stdout, write, yunta_in};

/// A node that fails until a person fixes its cause, so the run parks
/// and a later `resume` wakes it.
const PARKS: &str =
    "name: parks\nnodes:\n  - id: broken\n    kind: bash\n    run: \"test -f fixed.txt\"\n";

/// `yunta resume <run>` from a shell whose `PATH` starts with `extra`.
fn resume_with_path_gaining(repo: &Path, home: &Path, run: &str, extra: &Path) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yunta"));
    hermetic(&mut cmd, repo, home);
    cmd.env("PATH", yunta_testkit::stubs::path_with(extra))
        .args(["resume", run])
        .stdin(Stdio::null())
        .output()
        .expect("the yunta binary runs")
}

#[test]
fn status_says_when_a_wake_runs_commands_in_another_environment() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, extra) = (
        root.path().join("repo"),
        root.path().join("home"),
        root.path().join("tools"),
    );
    for dir in [&repo, &extra] {
        std::fs::create_dir_all(dir).unwrap();
    }
    init_repo(&repo);
    write(&repo.join("wf.yaml"), PARKS);
    let run = run_id_from(&yunta_in!(&repo, &home, &["run", "wf.yaml"]));

    resume_with_path_gaining(&repo, &home, &run, &extra);
    let status = stdout(&yunta_in!(&repo, &home, &["status", &run]));

    assert!(
        status.contains(&format!(
            "environment: commands now run in another environment than the run was born \
             in: PATH gained {}",
            extra.display()
        )),
        "{status}"
    );
}
