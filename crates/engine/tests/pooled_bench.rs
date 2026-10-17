//! A bench can run the way `yunta run` does: in a checkout of the
//! project's pool, taken on the run's own branch, and found again on a
//! wake as `yunta resume` finds it.

use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, git_output, Bench};

mod common;
use common::answer_parked;

const MAKES: &str =
    "name: makes\nnodes:\n  - id: make\n    kind: bash\n    run: \"echo made > made.txt\"\n";

const GATED: &str = "name: gated\nnodes:\n  - { id: approve, kind: gate, assignee: lead, options: [approve] }\n  - { id: make, kind: bash, depends_on: [approve], run: \"echo made > made.txt\" }\n";

#[tokio::test]
async fn a_pooled_bench_runs_in_a_checkout_of_the_projects_pool() {
    let bench = Bench::new().pooled();

    let RunReport { terminal, .. } = bench
        .run(MAKES, "dialect: claude-code\nsessions: []\n")
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let tree = tokio::fs::canonicalize(bench.tree()).await.unwrap();
    let pool = tokio::fs::canonicalize(bench.pool()).await.unwrap();
    assert_eq!(tree.parent(), Some(pool.as_path()));
    let made = format!("yunta/run/{}:made.txt", bench.run_id);
    assert_eq!(git_output(&bench.worktree, &["show", &made]).trim(), "made");
}

#[tokio::test]
async fn a_pooled_benchs_wake_takes_a_checkout_again_on_the_runs_branch() {
    let bench = Bench::new().pooled();
    let RunReport { terminal, .. } = bench
        .run(GATED, "dialect: claude-code\nsessions: []\n")
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    // Given back while the run was parked: on no branch, anybody's.
    git(&bench.tree(), &["switch", "-q", "--detach"]);
    answer_parked(&bench, "approve").await.unwrap();

    let RunReport { terminal, .. } = bench.wake().await;

    assert_eq!(terminal, RunTerminal::Finished);
    let made = format!("yunta/run/{}:made.txt", bench.run_id);
    assert_eq!(git_output(&bench.worktree, &["show", &made]).trim(), "made");
}
