//! Every degradation the engine hits is recorded on the run's log — a
//! `finding_posted` or a `capability_degraded`, never a bare `tracing`
//! warning that leaves the log silent about what the engine could not do.
//! These runs provoke each degradation deterministically and read the
//! event back off storage.

use yunta_core::events::FindingEvent;
use yunta_core::events::{EventPayload, Finding, StoredEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, Bench, MOCK_CONFIG};

/// The finding with `id`, or `None` — findings are the engine's own
/// record of a degradation, so a test asserts one is present by id.
fn finding(events: &[StoredEvent], id: &str) -> Option<Finding> {
    events.iter().find_map(|event| match event.payload() {
        Some(EventPayload::Findings(FindingEvent::Posted(p))) if p.finding.id.as_str() == id => {
            Some(p.finding.clone())
        }
        _ => None,
    })
}

#[tokio::test]
async fn an_unwritable_process_registry_is_recorded_as_a_finding() {
    // `create_run` makes `scratch/`; replacing `engine.json` with a
    // directory makes the registry's atomic write fail — the run must
    // record that its process tree became invisible from outside, not
    // warn and vanish.
    let bench = Bench::new();
    let workflow = r#"
name: degradation
nodes:
  - id: build
    kind: bash
    run: "true"
"#;
    let RunReport { terminal, .. } = bench
        .run_sabotaged(workflow, "sessions: []\n", |run_dir| {
            std::fs::create_dir_all(run_dir.join("scratch").join("engine.json")).unwrap();
        })
        .await;

    assert!(matches!(terminal, RunTerminal::Finished));
    let events = bench.events();
    let finding = finding(&events, "engine-registry")
        .expect("the unwritable registry must be recorded as a finding");
    assert!(
        finding.detail.contains("process tree"),
        "the finding says what became invisible: {}",
        finding.detail
    );
}

#[tokio::test]
async fn a_cleanup_on_a_primary_checkout_is_recorded_as_a_finding() {
    // `on_finish.cleanup: worktree` on a tree that is a primary checkout
    // (not a linked worktree) touches nothing — and says so with a
    // finding, never a warning only the operator's console would see.
    let bench = Bench::new();
    let workflow = r#"
name: degradation
nodes:
  - id: build
    kind: bash
    run: "true"
on_finish:
  - cleanup: worktree
"#;
    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []\n").await;

    assert!(matches!(terminal, RunTerminal::Finished));
    assert!(
        finding(&bench.events(), "cleanup-not-a-worktree").is_some(),
        "a cleanup that cannot run on a primary checkout must be a finding"
    );
}

#[tokio::test]
async fn a_failed_distill_commit_is_recorded_as_a_finding() {
    // A failing `pre-commit` hook makes distill's `git commit` fail; the
    // files stay on disk and the failure is a finding on the log.
    let bench = Bench::new();
    let hooks = bench.worktree.join(".git-hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let pre_commit = hooks.join("pre-commit");
    std::fs::write(&pre_commit, "#!/bin/sh\nexit 1\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&pre_commit, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(
        &bench.worktree,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );

    let workflow = r#"
name: degradation
nodes:
  - id: build
    kind: bash
    run: "true"
on_finish:
  - distill:
      - { node: build, name: notes.md }
"#;
    let RunReport { terminal, .. } = bench.run(workflow, "sessions: []\n").await;

    assert!(matches!(terminal, RunTerminal::Finished));
    assert!(
        finding(&bench.events(), "distill-commit").is_some(),
        "a distill commit blocked by a failing hook must be a finding"
    );
}

/// A fallback the whole run works under is stated once. The fence is
/// the run's condition, not a node's choice: every node with a declared
/// scope runs unfenced on an adapter that can build none, and repeating
/// that per node says nothing new and buries what does.
#[tokio::test]
async fn a_run_on_an_adapter_without_a_fence_says_so_once() {
    let bench = Bench::new();
    let workflow = r#"
name: two-scoped-nodes
nodes:
  - id: first
    kind: prompt
    runner: executor
    prompt: "Do the first thing."
    scope: ["a.txt"]
  - id: second
    kind: prompt
    runner: executor
    depends_on: [first]
    prompt: "Do the second thing."
    scope: ["b.txt"]
"#;
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "first" }
  - outcome: { type: completed, summary: "second" }
"#;
    let RunReport { terminal, .. } = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let stated = degradations_of(&bench, yunta_core::Capability::Fence);
    assert_eq!(
        stated.len(),
        1,
        "two scoped nodes, one adapter that cannot hold them: {stated:?}"
    );
    assert_eq!(
        stated[0],
        yunta_core::events::Policy::PostCheckOnly.to_string()
    );
}

/// A run whose adapter reports no usage cannot count what it spends, so
/// it neither hands sessions a cap it cannot enforce nor stays silent
/// about the cap it was given.
#[tokio::test]
async fn a_run_on_an_adapter_without_usage_reporting_says_it_has_no_token_budget() {
    let bench = Bench::new();
    let workflow = r#"
name: capped
nodes:
  - id: only
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#;
    let config = format!("{MOCK_CONFIG}\nlimits:\n  max_tokens_per_run: 100000\n");
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let RunReport { terminal, .. } = bench.run_with_config(workflow, fixture, &config).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let stated = degradations_of(&bench, yunta_core::Capability::UsageReporting);
    assert_eq!(stated.len(), 1, "{stated:?}");
    assert_eq!(
        stated[0],
        yunta_core::events::Policy::NoTokenBudget.to_string()
    );

    // And the cap it cannot count against never reaches a session.
    let budgets: Vec<Option<u64>> = bench
        .mock()
        .requests_seen()
        .into_iter()
        .map(|request| request.budget.max_tokens)
        .collect();
    assert_eq!(
        budgets,
        vec![None],
        "a run that cannot count tokens hands out no token cap"
    );
}

/// The `policy_applied` of every `capability_degraded` the run recorded
/// for `capability`, in log order.
fn degradations_of(bench: &Bench, capability: yunta_core::Capability) -> Vec<String> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Session(yunta_core::events::SessionEvent::CapabilityDegraded(
                p,
            ))) if p.capability == capability => Some(p.policy_applied().to_string()),
            _ => None,
        })
        .collect()
}

/// A secret the config names never reaches the log.
///
/// It reaches the session's environment on purpose, and the session may
/// then say it back — a note quoting a command line it ran, an error
/// repeating a URL with a token in it. The log is the run's permanent
/// record, so what the config called a secret is taken back out on the
/// way in, at the one door every event goes through.
#[tokio::test]
async fn a_secret_the_config_names_never_reaches_the_log() {
    const VALUE: &str = "hunter2-the-whole-token";

    let bench = Bench::new();
    let config = format!("{MOCK_CONFIG}\nsecrets: [YUNTA_TEST_TOKEN]\n");
    let workflow = r#"
name: leaky
nodes:
  - id: talk
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#;
    // The session says the secret back, twice over: once as a note, and
    // once as the outcome its close records.
    let fixture = format!(
        "sessions:\n  - steps:\n      - {{ type: note, text: \"ran psql with {VALUE}\" }}\n    outcome: {{ type: completed, summary: \"used {VALUE}\" }}\n"
    );

    let RunReport { .. } = bench
        .run_with_secrets(workflow, &fixture, &config, &[("YUNTA_TEST_TOKEN", VALUE)])
        .await;

    let log = format!("{:?}", bench.events());
    assert!(
        !log.contains(VALUE),
        "the secret reached the log:\n{}",
        log.lines()
            .filter(|line| line.contains(VALUE))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        log.contains(yunta_core::REDACTED),
        "and the log says where it was: {log}"
    );
}

/// An adapter that says it judged every write before it happened, and
/// a write that reached the diff anyway: that is something to look at in
/// the adapter, filed beside the failure the violation causes either way.
#[tokio::test]
async fn a_write_that_escapes_an_exact_fence_is_an_engine_finding() {
    let bench = Bench::new();
    let workflow = r#"
name: scoped
nodes:
  - id: work
    kind: prompt
    runner: executor
    prompt: "Do the thing."
    scope: ["a.txt"]
"#;
    // A fixture that reports an exact fence and writes outside it
    // anyway: the hook timed out, which this CLI lets through.
    let fixture = r#"
capabilities: { fence: none }
fence_coverage: exact
effects:
  - { path: b.txt, content: "outside the scope\n" }
outcome: { type: completed, summary: "done" }
"#;
    let RunReport { terminal, .. } = bench.run(workflow, fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "the violation fails the node and pauses the run: {terminal:?}"
    );

    assert!(
        breaches(&bench).len() == 1,
        "one breach, naming what escaped: {:?}",
        breaches(&bench)
    );
    assert!(
        breaches(&bench)[0].detail.contains("b.txt"),
        "the finding names the path that reached the diff: {:?}",
        breaches(&bench)[0]
    );
}

/// Under a widened or tool-only coverage a violation is what it always
/// was: the node fails with the list, and nobody claimed more.
#[tokio::test]
async fn a_write_inside_widened_roots_is_a_scope_violation_and_nothing_more() {
    let bench = Bench::new();
    let workflow = r#"
name: scoped
nodes:
  - id: work
    kind: prompt
    runner: executor
    prompt: "Do the thing."
    scope: ["a.txt"]
"#;
    let fixture = r#"
capabilities: { fence: filesystem }
effects:
  - { path: b.txt, content: "outside the scope\n" }
outcome: { type: completed, summary: "done" }
"#;
    let RunReport { terminal, .. } = bench.run(workflow, fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "the violation fails the node and pauses the run: {terminal:?}"
    );
    assert!(
        breaches(&bench).is_empty(),
        "a sandbox never claimed to judge by glob: {:?}",
        breaches(&bench)
    );
}

/// Every fence breach the run filed.
fn breaches(bench: &Bench) -> Vec<Finding> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Findings(FindingEvent::Posted(p)))
                if p.finding.id == yunta_engine::Breach::ID =>
            {
                Some(p.finding.clone())
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn an_engine_finding_locates_where_the_door_can_read() {
    // The engine's own findings and an agent's are the same thing on the
    // log, and a successor run inherits both through the one findings
    // document. So what the engine writes passes the door that document
    // parses through, and names the root it is under: the process
    // registry is the run's own bookkeeping, the worktree a cleanup
    // could not remove is the work itself.
    let bench = Bench::new();
    let workflow = r#"
name: degradation
nodes:
  - id: build
    kind: bash
    run: "true"
on_finish:
  - cleanup: worktree
"#;
    let RunReport { terminal, .. } = bench
        .run_sabotaged(workflow, "sessions: []\n", |run_dir| {
            std::fs::create_dir_all(run_dir.join("scratch").join("engine.json")).unwrap();
        })
        .await;

    assert!(matches!(terminal, RunTerminal::Finished));
    let events = bench.events();

    assert_eq!(
        finding(&events, "engine-registry")
            .expect("the registry finding")
            .location,
        "run:scratch/engine.json".into(),
        "the registry is the run's own, not this host's"
    );
    assert_eq!(
        finding(&events, "cleanup-not-a-worktree")
            .expect("the cleanup finding")
            .location,
        ".".into(),
        "a tree that stays is the work, whatever absolute path holds it"
    );

    let posted: Vec<Finding> = events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Findings(FindingEvent::Posted(p))) => Some(p.finding.clone()),
            _ => None,
        })
        .collect();
    assert!(posted.len() >= 2, "this run posts both findings");

    let document = yunta_core::FindingsFile::from_findings(posted.clone());
    let yaml = serde_norway::to_string(&document).expect("a findings document serializes");
    let read = yunta_core::shape::read::<yunta_core::FindingsFile>(
        yaml.as_bytes(),
        "artifacts/findings.yaml",
    )
    .expect("every engine finding reads back through the door a successor inherits it by");
    assert_eq!(
        read.findings
            .iter()
            .map(|entry| entry.location.clone())
            .collect::<Vec<_>>(),
        posted
            .iter()
            .map(|finding| finding.location.clone())
            .collect::<Vec<_>>(),
    );
}
