//! The lineage's measurement taken aside. A run born in a checkout holding
//! exactly the commit it opened on measures its suite in a checkout of its
//! pool while its first nodes run; an invocation that ends before the suite
//! answers records nothing, and the next wake measures again in the same
//! checkout, on what the stopped suite already built.

use yunta_core::events::{EventPayload, NodeEvent, RunEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{ApproveEverything, Bench, MOCK_CONFIG};

/// Commits a `.gitignore` that ignores `target/`, so the run opens on a
/// clean tree and a build's output survives a checkout's reuse.
fn ignore_builds(bench: &Bench) {
    std::fs::write(bench.worktree.join(".gitignore"), "target/\n").unwrap();
    yunta_testkit::git(&bench.worktree, &["add", "-A"]);
    yunta_testkit::git(&bench.worktree, &["commit", "-q", "-m", "ignore builds"]);
}

/// How many measurements the log holds.
fn measurements(bench: &Bench) -> usize {
    bench
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Run(RunEvent::BaselineCaptured(_)))
            )
        })
        .count()
}

/// The first wake parks on a gate while the suite is still building —
/// `wait` holds the gate until the build has begun — and records nothing;
/// the next wake measures in the same checkout and finds the build there.
#[tokio::test]
async fn a_measurement_an_invocation_stopped_starts_again_on_its_warm_checkout() {
    let bench = Bench::new();
    ignore_builds(&bench);
    let warm = bench.run_dir().join("unit-worktrees/slot-1/target/warm");
    let said = bench.run_dir().with_extension("suite-said");
    let suite = format!(
        "if test -f target/warm; then echo warm >> {said}; else mkdir -p target; touch target/warm; \
         sleep 30; fi",
        said = said.display()
    );
    let config = format!("{MOCK_CONFIG}baseline:\n  suite: \"{suite}\"\n");
    let workflow = format!(
        r#"
name: parked-while-measuring
nodes:
  - id: wait
    kind: bash
    run: "for i in $(seq 1 600); do test -f {warm} && break; sleep 0.05; done; test -f {warm}"
  - {{ id: approve, kind: gate, assignee: lead, depends_on: [wait] }}
  - {{ id: regressions, kind: check, builtin: baseline_compare, depends_on: [approve] }}
"#,
        warm = warm.display()
    );

    let RunReport { terminal, .. } = bench
        .run_with_config(&workflow, "sessions: []", &config)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        measurements(&bench),
        0,
        "a suite nobody let finish measured nothing"
    );

    let RunReport { terminal, .. } = bench.wake_answering(&ApproveEverything::new("test")).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(measurements(&bench), 1);
    let said = tokio::fs::read_to_string(&said).await.unwrap();
    assert_eq!(said.lines().collect::<Vec<_>>(), vec!["warm"]);
}

/// A run working in a person's own checkout measures there, before any
/// node of its own can change the tree.
#[tokio::test]
async fn a_run_without_a_tree_of_its_own_measures_before_its_first_node() {
    let bench = Bench::new();
    let config =
        format!("{MOCK_CONFIG}defaults:\n  isolation: none\nbaseline:\n  suite: \"true\"\n");
    let workflow = "\
name: in-place
nodes:
  - { id: work, kind: bash, run: \"true\" }
  - { id: regressions, kind: check, builtin: baseline_compare, depends_on: [work] }
";

    let RunReport { terminal, .. } = bench
        .run_with_config(workflow, "sessions: []", &config)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let first = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Run(RunEvent::BaselineCaptured(_))) => Some("measured"),
            Some(EventPayload::Node(NodeEvent::Started(_))) => Some("a node started"),
            _ => None,
        });
    assert_eq!(first, Some("measured"));
}

/// A measurement that fails on its way to the log leaves the node beside
/// it running to its own end: the node closes on the log, and the failure
/// comes after. The suite plants a file where its capture's directory goes.
#[tokio::test]
async fn a_measurement_error_does_not_drop_the_node_beside_it() {
    let bench = Bench::new();
    let blocked = bench.run_dir().join("baseline");
    let finished = bench.run_dir().with_extension("finished");
    let config = format!(
        "{MOCK_CONFIG}baseline:\n  suite: \"printf x > {}\"\n",
        blocked.display()
    );
    let workflow = format!(
        r#"
name: measured-beside
nodes:
  - {{ id: work, kind: bash, run: "sleep 2; touch {finished}" }}
  - {{ id: regressions, kind: check, builtin: baseline_compare, depends_on: [work] }}
"#,
        finished = finished.display()
    );
    bench.create(&workflow, "sessions: []", &config).await;

    let woke = bench.try_wake().await;

    assert!(woke.is_err(), "{woke:?}");
    assert!(
        finished.exists(),
        "the node beside the measurement ran to its end"
    );
    let closed = bench.events().iter().any(|event| {
        event
            .node_id
            .as_ref()
            .is_some_and(|node| node.as_str() == "work")
            && matches!(
                event.payload(),
                Some(EventPayload::Node(NodeEvent::Finished(_)))
            )
    });
    assert!(closed, "its close is on the log");
}
