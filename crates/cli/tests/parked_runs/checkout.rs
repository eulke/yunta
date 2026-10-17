//! A session the engine could not stop keeps its checkout taken until the run is
//! cancelled, and the cancel hands it back.

use super::*;

/// A node whose session the engine cannot stop: it spends past the run's
/// token cap, and its kill is refused while it plays. The node declares its
/// scope, so it works in a checkout of its own.
const UNSTOPPABLE: &str = r#"
name: unstoppable
nodes:
  - id: work
    kind: prompt
    runner: executor
    scope: ["out.txt"]
    prompt: "Write out.txt."
"#;

/// The session behind [`UNSTOPPABLE`]: it reports 200 tokens against a cap of
/// 100, then hangs, ignoring the interrupt, so the engine must stop it itself.
const UNSTOPPABLE_FIXTURE: &str = r#"
dialect: claude-code
sessions:
  - kill: refuse
    steps:
      - { type: usage, input_tokens: 200, output_tokens: 0 }
    outcome: { type: hang, on_interrupt: ignore }
"#;

/// The session of a second run over the same repository: it spends one
/// token and completes, so the checkout it takes is the one given back.
const FINISHING_FIXTURE: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: usage, input_tokens: 1, output_tokens: 0 }
    outcome: { type: completed, summary: done }
"#;

/// The checkout locks of every pool under the state root `home`.
fn slot_locks(home: &Path) -> Vec<PathBuf> {
    let Ok(pools) = std::fs::read_dir(home.join("worktrees").join("pool")) else {
        return Vec::new();
    };
    pools
        .flatten()
        .filter_map(|pool| std::fs::read_dir(pool.path()).ok())
        .flat_map(|slots| slots.flatten().map(|slot| slot.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "lock"))
        .collect()
}

/// One `yunta run` of the unstoppable workflow, under the fixture `fixture`.
fn run_unstoppable(repo: &Path, home: &Path, fixture: &str) -> std::process::Output {
    yunta_in!(
        repo,
        home,
        &[
            "run",
            "unstoppable.yaml",
            "--adapter",
            "mock",
            "--fixture",
            fixture
        ]
    )
}

#[test]
fn a_parked_run_whose_session_could_not_stop_keeps_its_checkout_until_cancel() {
    let root = tempfile::tempdir().unwrap();
    let repo = repo_with(
        root.path(),
        &[
            ("unstoppable", UNSTOPPABLE),
            ("fixture", UNSTOPPABLE_FIXTURE),
            ("finish", FINISHING_FIXTURE),
        ],
    );
    let home = root.path().join("state");
    write(
        &repo.join(".yunta/config.yaml"),
        &format!("{MOCK_CONFIG}limits:\n  max_tokens_per_run: 100\n"),
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "runners"]);

    let parked = run_unstoppable(&repo, &home, "fixture.yaml");
    let run_id = run_id_from(&parked);
    assert!(
        !slot_locks(&home).is_empty(),
        "the checkout stays taken once the run parks: {}",
        stdout(&parked)
    );

    yunta_in!(&repo, &home, &["cancel", &run_id]);
    // The cancel gives back the checkouts the parked run worked in; the next
    // run takes those, and makes no checkout of its own.
    let freed = yunta_testkit::pool_checkouts(&home);
    assert!(!freed.is_empty(), "the parked run worked in a checkout");
    let next = run_unstoppable(&repo, &home, "finish.yaml");
    assert_eq!(
        yunta_testkit::pool_checkouts(&home),
        freed,
        "the next run takes the checkouts the cancel freed, not new ones: {}",
        stdout(&next)
    );
}
