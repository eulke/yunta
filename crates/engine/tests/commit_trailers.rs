//! Every commit a run makes names where it came from: the run, and the
//! node and the task whose work it holds, as git trailers a clone reads
//! without the run's log.

use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git_output, Bench};

/// A node that leaves a file in the run's tree, one that works in a
/// checkout of its own, and a loop whose task lands its work: each way
/// the engine commits.
const EVERY_COMMIT: &str = r#"
name: every-commit
nodes:
  - id: write
    kind: bash
    run: "echo made > made.txt"
  - id: land
    kind: bash
    depends_on: [write]
    scope: [landed.txt]
    run: "echo landed > landed.txt"
  - id: plan
    kind: prompt
    runner: planner
    depends_on: [land]
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

const SESSIONS: &str = r#"
capabilities:
  run_tools: true
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            tasks:
              - id: T001
                title: Create hello
                scope: [hello.txt]
                criteria:
                  - cmd: "test -f hello.txt"
    outcome: { type: completed, summary: planned }
  - effects:
      - { path: hello.txt, content: hello }
    outcome: { type: completed, summary: did T001 }
"#;

/// Every commit the repository holds that the run made — all but the
/// one it started on — as `(subject, trailers)`.
fn run_commits(bench: &Bench) -> Vec<(String, String)> {
    git_output(&bench.worktree, &["rev-list", "--all", "--min-parents=1"])
        .lines()
        .map(|sha| {
            let subject = git_output(&bench.worktree, &["log", "-1", "--format=%s", sha]);
            let trailers = git_output(
                &bench.worktree,
                &["log", "-1", "--format=%(trailers:only,unfold)", sha],
            );
            (subject.trim().to_string(), trailers.trim().to_string())
        })
        .collect()
}

#[tokio::test]
async fn every_engine_commit_carries_the_run_trailer() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(EVERY_COMMIT, SESSIONS).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let commits = run_commits(&bench);
    let run = format!("Yunta-Run: {}", bench.run_id);
    for (subject, trailers) in &commits {
        assert!(
            trailers.lines().any(|line| line == run),
            "`{subject}` does not name its run: {trailers:?}"
        );
    }
    let trailers_of = |prefix: &str| {
        commits
            .iter()
            .find(|(subject, _)| subject.starts_with(prefix))
            .map(|(_, trailers)| trailers.clone())
            .unwrap_or_else(|| panic!("no commit `{prefix}…` in {commits:#?}"))
    };
    assert!(trailers_of("node write").contains("Yunta-Node: write"));
    assert!(trailers_of("node land").contains("Yunta-Node: land"));
    let task = trailers_of("task T001");
    assert!(task.contains("Yunta-Node: implement"), "{task}");
    assert!(task.contains("Yunta-Task: T001"), "{task}");
}

#[tokio::test]
async fn git_log_grep_finds_a_run_s_commits() {
    let bench = Bench::new();
    bench.run(EVERY_COMMIT, SESSIONS).await;

    let found = git_output(
        &bench.worktree,
        &[
            "log",
            "--all",
            "--format=%s",
            &format!("--grep=^Yunta-Run: {}$", bench.run_id),
        ],
    );
    assert_eq!(
        found.lines().count(),
        run_commits(&bench).len(),
        "every commit the run made is found by its run: {found}"
    );
    assert!(found
        .lines()
        .any(|subject| subject.starts_with("task T001")));
}
