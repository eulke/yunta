//! A run's criteria keep their answers across wakes. A check whose tree
//! and environment did not change since an earlier invocation answered it
//! is not run again only because that invocation ended; a wake whose
//! commands run elsewhere runs it again.

use yunta_core::events::{EventPayload, NodeEvent, Phase};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{ApproveEverything, Bench, MOCK_CONFIG};

/// A plan that writes a one-task document, a gate that parks the run
/// before the loop that works it.
const PLANNED_THEN_GATED: &str = r#"
name: planned-then-gated
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: Make it\n    scope: [made.txt]\n    criteria:\n      - cmd: test -f made.txt\n' > {{node.artifacts}}/tasks.yaml"
    artifacts:
      produces: [tasks]
  - { id: approve, kind: gate, assignee: lead, depends_on: [plan] }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [approve]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

const MAKES_IT: &str = "\
capabilities: { run_tools: true }
sessions:
  - effects:
      - { path: made.txt, content: made }
    outcome: { type: completed, summary: made }
";

/// A suite that counts its runs in a file outside the tree it judges.
fn config(bench: &Bench) -> (String, std::path::PathBuf) {
    let runs = bench.run_dir().with_extension("suite-runs");
    let suite = format!("echo ran >> {}; true", runs.display());
    (
        format!("{MOCK_CONFIG}baseline:\n  suite: \"{suite}\"\n"),
        runs,
    )
}

/// The commands' `PATH`: this process's own, with `first` ahead of it.
fn path(first: &std::path::Path) -> Vec<(String, String)> {
    let path = yunta_testkit::stubs::path_with(first)
        .into_string()
        .expect("a PATH that is text");
    vec![("PATH".to_string(), path)]
}

/// Whether every pre-check of T001 took the suite's answer from the memo.
fn pre_checks_reused(bench: &Bench) -> Vec<bool> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(p))) if p.phase == Phase::Pre => {
                Some(
                    p.results
                        .iter()
                        .filter(|r| r.r#type.is_some())
                        .map(|r| r.reused)
                        .collect::<Vec<_>>(),
                )
            }
            _ => None,
        })
        .flatten()
        .collect()
}

/// The first wake measures the suite and parks on the gate; the next
/// takes the measurement's answer for the tree the task starts from.
#[tokio::test]
async fn a_wake_takes_what_an_earlier_invocation_answered_for_the_same_tree() {
    let here = tempfile::tempdir().unwrap();
    let bench = Bench::new().with_subprocess_vars(path(here.path()));
    let (config, runs) = config(&bench);
    let RunReport { terminal, .. } = bench
        .run_with_config(PLANNED_THEN_GATED, MAKES_IT, &config)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let RunReport { terminal, .. } = bench.wake_answering(&ApproveEverything::new("test")).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(pre_checks_reused(&bench), vec![true]);
    let ran = tokio::fs::read_to_string(&runs).await.unwrap();
    assert_eq!(
        ran.lines().count(),
        2,
        "measured once, and once on the work"
    );
}

/// A wake whose commands look programs up elsewhere takes nothing an
/// earlier invocation answered: the same command may be another program.
#[tokio::test]
async fn a_wake_whose_commands_run_elsewhere_runs_them_again() {
    let here = tempfile::tempdir().unwrap();
    let bench = Bench::new().with_subprocess_vars(path(here.path()));
    let (config, runs) = config(&bench);
    let RunReport { terminal, .. } = bench
        .run_with_config(PLANNED_THEN_GATED, MAKES_IT, &config)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let elsewhere = tempfile::tempdir().unwrap();
    let bench = bench.with_subprocess_vars(path(elsewhere.path()));
    let RunReport { terminal, .. } = bench.wake_answering(&ApproveEverything::new("test")).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(pre_checks_reused(&bench), vec![false]);
    let ran = tokio::fs::read_to_string(&runs).await.unwrap();
    assert_eq!(
        ran.lines().count(),
        3,
        "measured, checked again, and on the work"
    );
}
