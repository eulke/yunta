//! Promotion at the engine boundary: the escalation
//! at exhausted re-routes offers a `promote` option exactly when a
//! later mode exists (the modes' declaration order forms a ladder), and
//! choosing it closes *this* run for good — `run_finished` with
//! `terminal_state: Promoted`, never reopened — after recording
//! `promotion_signaled` on the same log. Actually creating and running
//! the successor is `yunta-cli`'s own job (`commands/promote.rs`) —
//! `execute_run` alone only has an *already-prepared* worktree, never
//! the original checkout a fresh one needs — so this suite only proves
//! the parent-side half of the chain.

use std::collections::HashMap;
use std::sync::Arc;

use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::events::EventPayload;
use yunta_core::SeqIdSource;
use yunta_core::{AdapterId, ConfigLayer, ModeName, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, HumanInteraction, NoInteraction,
    RunEnv, RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_storage::Storage;
use yunta_testkit::{init_repo, FixedClock, ScriptedInteraction};

/// Run ids for everything a test run gives birth to — unique across
/// the binary, so parallel tests never share a run directory.
static IDS: SeqIdSource = SeqIdSource::new("minted");

const CONFIG: &str = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
"#;

/// `lint` fails immediately (`test -f` on a file nothing ever creates)
/// with `max_reroutes: 0` — its very first failure already exhausts
/// re-routes, so the escalation gate fires on the first wake.
const PROMOTABLE_WORKFLOW: &str = r#"
name: promotable
modes:
  quick:  { include: [lint, fix-lint] }
  full:   { include: all }
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "true"
"#;

const NO_LATER_MODE_WORKFLOW: &str = r#"
name: promotable
modes:
  full:   { include: all }
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "true"
"#;

async fn run_with_mode(
    workflow_yaml: &str,
    mode: &str,
    interaction: &dyn HumanInteraction,
) -> (RunTerminal, Vec<yunta_core::events::StoredEvent>) {
    let (terminal, events, _run_dir, _root) =
        run_with_mode_and_findings(workflow_yaml, mode, interaction, &[]).await;
    (terminal, events)
}

/// Same, but with engine findings planted on the log after creation (the
/// scenario where a scope-expansion denial lives only in the parent's events), and
/// the run dir returned so tests can inspect derived artifacts.
async fn run_with_mode_and_findings(
    workflow_yaml: &str,
    mode: &str,
    interaction: &dyn HumanInteraction,
    findings: &[yunta_core::events::Finding],
) -> (
    RunTerminal,
    Vec<yunta_core::events::StoredEvent>,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let root = tempfile::tempdir().unwrap();
    let worktree = root.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    init_repo(&worktree);
    let runs_root = root.path().join("runs");
    let storage = Storage::open(&root.path().join("yunta.db")).unwrap();
    let run_id = RunId::from("run-promo-1");

    let workflow: Workflow = serde_norway::from_str(workflow_yaml).unwrap();
    let config: ConfigLayer = serde_norway::from_str(CONFIG).unwrap();
    let manifest =
        build_manifest(&workflow, &config, &worktree, &worktree, &HashMap::new()).unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &run_id,
            manifest: &manifest,
            runs_root: &runs_root,
            mode: &ModeName::from(mode),
            promoted_from: None,
            artifacts: &[],
        },
        &storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    for finding in findings {
        storage
            .append(
                &yunta_core::events::EventDraft {
                    run_id: run_id.clone(),
                    node_id: None,
                    payload: EventPayload::FindingPosted(
                        yunta_core::events::FindingPostedPayload {
                            finding: finding.clone(),
                        },
                    ),
                },
                &yunta_core::SystemClock,
            )
            .unwrap();
    }

    let adapter = MockAdapter::from_yaml("sessions: []\n").unwrap();
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), Arc::new(adapter));

    let report = execute_run(RunEnv {
        run_id: &run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &worktree,
        adapters: &adapters,
        storage: &storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: interaction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();

    let events = storage.events_for_run(&run_id).unwrap();
    (report.terminal, events, run_dir, root)
}

#[tokio::test]
async fn promote_is_offered_and_closes_the_run_with_promotion_signaled() {
    let interaction = ScriptedInteraction::choose("promote");
    let (terminal, events) = run_with_mode(PROMOTABLE_WORKFLOW, "quick", &interaction).await;

    match &terminal {
        RunTerminal::Promoted { suggested_mode } => assert_eq!(suggested_mode, "full"),
        other => panic!("expected Promoted, got {other:?}"),
    }

    let signaled = events.iter().find_map(|e| match e.payload() {
        Some(EventPayload::PromotionSignaled(p)) => Some(p),
        _ => None,
    });
    let signaled = signaled.expect("promotion_signaled must be on the parent's own log");
    assert_eq!(signaled.suggested_mode, "full");
    assert!(!signaled.reason.is_empty());

    let finished = events.iter().find_map(|e| match e.payload() {
        Some(EventPayload::RunFinished(p)) => Some(p),
        _ => None,
    });
    assert_eq!(
        finished.map(|p| p.terminal_state),
        Some(yunta_core::events::TerminalState::Promoted)
    );

    // Promoting closes the run for good — no further events after
    // run_finished (nothing reopens a finished run).
    let last = events.last().unwrap();
    assert!(matches!(last.payload(), Some(EventPayload::RunFinished(_))));

    // The offered options actually included "promote" — proving the
    // escalation added it, not that this test just got lucky with a
    // fallback.
    let options = interaction.seen_options();
    assert!(
        options[0].contains(&"promote".to_string()),
        "got: {options:?}"
    );
}

#[tokio::test]
async fn promote_is_never_offered_with_no_later_mode() {
    let interaction = ScriptedInteraction::choose("abort");
    let (terminal, _events) = run_with_mode(NO_LATER_MODE_WORKFLOW, "full", &interaction).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));

    let options = interaction.seen_options();
    assert!(
        !options[0].contains(&"promote".to_string()),
        "the last declared mode has nowhere to promote to — got: {options:?}"
    );
}

#[tokio::test]
async fn without_a_live_human_interaction_the_run_just_pauses_never_promotes() {
    let (terminal, events) = run_with_mode(PROMOTABLE_WORKFLOW, "quick", &NoInteraction).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.payload(), Some(EventPayload::PromotionSignaled(_)))),
        "no live surface to choose promote from — must never happen on its own"
    );
}

// --- findings survive promotion ---------------------------------------

fn finding(id: &str, title: &str, location: &str) -> yunta_core::events::Finding {
    yunta_core::events::Finding {
        id: id.into(),
        severity: yunta_core::events::FindingSeverity::Major,
        title: title.to_string(),
        location: location.to_string(),
        detail: "scope expansion denied by a human".to_string(),
        proposed_criterion: None,
    }
}

#[tokio::test]
async fn a_promoting_run_derives_findings_inherited_for_its_successor() {
    let interaction = ScriptedInteraction::choose("promote");
    let planted = [
        finding(
            "scope-expansion-T001-1",
            "Scope expansion denied",
            "tasks/T001",
        ),
        // Same location + same title modulo case/whitespace: a duplicate
        // under the normative dedup rule.
        finding(
            "scope-expansion-T001-2",
            "scope  expansion DENIED",
            "tasks/T001",
        ),
        finding(
            "scope-expansion-T002-1",
            "Scope expansion denied",
            "tasks/T002",
        ),
    ];
    let (terminal, _events, run_dir, _root) =
        run_with_mode_and_findings(PROMOTABLE_WORKFLOW, "quick", &interaction, &planted).await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));

    let path = run_dir.join("artifacts/findings-inherited.yaml");
    let bytes = std::fs::read(&path).expect("the promotion close must derive the file");
    let file: yunta_core::FindingsFile = serde_norway::from_slice(&bytes).unwrap();
    assert!(
        yunta_engine::register_findings(&file).is_empty(),
        "the derived file must satisfy the findings-file parser"
    );
    assert_eq!(file.findings.len(), 2, "duplicates collapse: {file:?}");
    assert_eq!(file.findings[0].id, "scope-expansion-T001-1");
    assert_eq!(file.findings[1].id, "scope-expansion-T002-1");
}

#[tokio::test]
async fn a_promoting_run_with_no_findings_writes_no_inherited_file() {
    let interaction = ScriptedInteraction::choose("promote");
    let (terminal, _events, run_dir, _root) =
        run_with_mode_and_findings(PROMOTABLE_WORKFLOW, "quick", &interaction, &[]).await;
    assert!(matches!(terminal, RunTerminal::Promoted { .. }));
    assert!(
        !run_dir.join("artifacts/findings-inherited.yaml").exists(),
        "no findings, no file — zero noise"
    );
}
