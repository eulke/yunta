//! `yunta stats` end-to-end (T7.5, §8.4/§8.6): the terminal view stays
//! inside 80 columns and never emits ANSI color codes, and the prior
//! estimation only appears once a workflow has at least three finished
//! runs — both driven through the real compiled binary, same style
//! `run_flow.rs` uses.

use std::path::Path;
use std::process::Output;

fn yunta_in(dir: &Path, home: &Path, args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(args)
        .current_dir(dir)
        .env("YUNTA_HOME", home)
        .output()
        .expect("failed to run the yunta binary")
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join(".gitkeep"), "").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn run_id_from(output: &Output) -> String {
    stdout(output)
        .lines()
        .find_map(|line| {
            line.strip_prefix("run ")
                .and_then(|rest| rest.split(':').next())
                .map(str::to_string)
        })
        .expect("run id in output")
}

fn bash_only_workflow() -> &'static str {
    r#"
name: bash-only-stats
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
  - id: verify
    kind: bash
    depends_on: [touch]
    run: "test -f made.txt"
"#
}

#[test]
fn stats_run_output_stays_inside_80_columns_and_never_uses_ansi_color() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(&repo.join("wf.yaml"), bash_only_workflow());

    let run = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_id = run_id_from(&run);

    let stats = yunta_in(&repo, &home, &["stats", &run_id]);
    assert!(
        stats.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&stats.stderr)
    );
    let text = stdout(&stats);
    assert!(text.contains("CPTV"), "got: {text}");

    for line in text.lines() {
        assert!(
            line.chars().count() <= 80,
            "line exceeds 80 columns ({} chars): {line:?}",
            line.chars().count()
        );
        assert!(
            !line.contains('\x1b'),
            "line uses an ANSI escape code, not colorless: {line:?}"
        );
    }
}

#[test]
fn prior_estimation_only_appears_once_three_runs_exist() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(&repo.join("wf.yaml"), bash_only_workflow());
    // `list_workflows` only ever looks under `.yunta/workflows/` —
    // separate from the path `yunta run` was given, same convention
    // `run_flow.rs`'s own list-based tests use.
    write(&repo.join(".yunta/workflows/wf.yaml"), bash_only_workflow());

    // Run 1: no history yet at all.
    let first = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(first.status.success());
    assert!(
        !stdout(&first).contains("past run"),
        "got: {}",
        stdout(&first)
    );

    // Run 2: only 1 finished run behind it — still below the floor of 3.
    let second = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(second.status.success());
    assert!(
        !stdout(&second).contains("past run"),
        "got: {}",
        stdout(&second)
    );

    // Run 3: only 2 finished runs behind it — still below the floor.
    let third = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(third.status.success());
    assert!(
        !stdout(&third).contains("past run"),
        "got: {}",
        stdout(&third)
    );

    // Run 4: now 3 finished runs are behind it — the floor is met.
    let fourth = yunta_in(&repo, &home, &["run", "wf.yaml"]);
    assert!(fourth.status.success());
    assert!(
        stdout(&fourth).contains("past run(s)"),
        "got: {}",
        stdout(&fourth)
    );

    // `yunta list` (catalog view) shows the same estimation once earned.
    let list = yunta_in(&repo, &home, &["list"]);
    assert!(list.status.success());
    assert!(
        stdout(&list).contains("past run(s)"),
        "got: {}",
        stdout(&list)
    );

    // `stats --workflow` on the other hand reports history from any
    // count >= 1 — only its *estimation* line gates on the floor.
    let workflow_stats = yunta_in(&repo, &home, &["stats", "--workflow", "bash-only-stats"]);
    assert!(workflow_stats.status.success());
    let text = stdout(&workflow_stats);
    assert!(text.contains("4 run(s)"), "got: {text}");
    assert!(text.contains("past run(s)"), "got: {text}");
}

#[test]
fn stats_workflow_with_no_runs_says_so_without_failing() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // A storage root must exist for `stats --workflow` to open — running
    // `doctor` (harmless, no adapters configured) is enough to create it
    // the same way any real first command would.
    let _ = yunta_in(&repo, &home, &["doctor"]);

    let result = yunta_in(&repo, &home, &["stats", "--workflow", "never-run"]);
    assert!(result.status.success());
    assert!(stdout(&result).contains("no runs of workflow"));
}

#[test]
fn stats_needs_a_run_id_or_workflow_flag() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let result = yunta_in(&repo, &home, &["stats"]);
    assert!(!result.status.success());
}
