use std::sync::Arc;

use yunta_core::events::{EventPayload, HumanChoice, RunEvent};
use yunta_core::{ConfigLayer, Env, ProcessSecrets, SystemIdSource};
use yunta_testkit::{Bench, RecordingObserver};

use super::*;
use crate::Workspace;

struct AlwaysPromote;

#[async_trait::async_trait]
impl HumanInteraction for AlwaysPromote {
    async fn resolve(&self, _e: &GateWaitingPayload) -> Option<HumanChoice> {
        Some(HumanChoice {
            option: "promote".into(),
            by: "eulke".into(),
            free_text: None,
        })
    }
}

/// `plan` produces a real artifact before `lint` sends the run to its
/// escalation, so the successor has something to inherit.
const WORKFLOW: &str = r#"
name: promotable
modes:
  alpha:  { include: [plan, lint, fix-lint] }
  beta:   { include: [ship] }
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks: []\n' > {{node.artifacts}}/plan.yaml"
    artifacts: { produces: [plan.yaml] }
  - id: lint
    kind: bash
    depends_on: [plan]
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "true"
  - id: ship
    kind: bash
    run: "true"
"#;

/// A mock fixture with no sessions: every node of the workflow is `bash`.
const FIXTURE: &str = "dialect: claude-code\nsessions: []\n";

/// A run promoted by its own gate, and its chain followed by the drive: the
/// bench that made the parent, the observer that watched the whole chain,
/// the parent's id, and the run the chase ended on with how it ended.
struct Chased {
    bench: Bench,
    recorder: Arc<RecordingObserver>,
    parent: RunId,
    last: RunId,
    report: RunReport,
}

/// The parent is created in `alpha` and woken once, as a bench run is: its
/// wake answers the promotion gate with `promote`. The successors are made
/// under the bench's runs root, so the project names that root as its own,
/// and the chain reads the store through the bench's handle.
async fn chased() -> Chased {
    let recorder = RecordingObserver::new();
    let bench = Bench::with_run_id("run-parent")
        .in_mode("alpha")
        .with_observer(recorder.clone());
    let report = bench
        .run_with_interaction(WORKFLOW, FIXTURE, &AlwaysPromote)
        .await;
    assert!(matches!(report.terminal, RunTerminal::Promoted { .. }));
    let root = tempfile::tempdir().unwrap();
    let project = Project {
        config: ConfigLayer::default(),
        runs_root: bench.runs_root.clone(),
        worktrees_root: root.path().join("worktrees"),
        storage_path: root.path().join("yunta.db"),
        user_root: root.path().to_path_buf(),
        workspace: Workspace {
            dir: bench.worktree.clone(),
            env: Env::default(),
            clock: Arc::new(yunta_testkit_core::FixedClock),
            ids: Arc::new(SystemIdSource),
            secrets: Arc::new(ProcessSecrets),
            binary: PathBuf::from("yunta"),
        },
    };
    let means = Means {
        adapters: Adapters::new(),
        forge: None,
        storage: bench.storage.async_handle(),
        ports: Ports {
            person: &AlwaysPromote,
            observer: Some(recorder.clone()),
        },
        ambient: &project.workspace.env,
    };
    let parent = bench.run_id.clone();
    let chain = (parent.clone(), bench.manifest(), bench.tree(), report);
    let (last, _, _, report) = project
        .promotions(chain, &means, &Stop::new())
        .await
        .unwrap();
    Chased {
        bench,
        recorder,
        parent,
        last,
        report,
    }
}

#[tokio::test]
async fn a_promotion_is_followed_onto_a_successor_of_its_own_to_its_end() {
    let chased = chased().await;
    assert_ne!(
        chased.last, chased.parent,
        "the successor is a run of its own"
    );
    assert_eq!(chased.report.terminal, RunTerminal::Finished);
    // The artifact the parent's log holds landed in the successor's own dir,
    // with no producer of the successor's behind it.
    let successor_dir = chased.bench.runs_root.join(chased.last.as_str());
    let plan = yunta_core::run_dir::artifacts(&successor_dir).join("plan.yaml");
    assert_eq!(std::fs::read_to_string(plan).unwrap(), "tasks: []\n");
}

/// The chain is audited on both logs: the successor's `run_created` names the
/// parent it was promoted from, and the parent's log signals the promotion.
#[tokio::test]
async fn a_promotion_is_audited_on_both_logs() {
    let chased = chased().await;
    let storage = &chased.bench.storage;
    let created = storage
        .events_for_run(&chased.last)
        .unwrap()
        .into_iter()
        .find_map(|e| match e.payload() {
            Some(EventPayload::Run(RunEvent::Created(p))) => Some(p.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(created.promoted_from, Some(chased.parent.clone()));
    assert_eq!(created.mode, "beta");
    let parent_events = storage.events_for_run(&chased.parent).unwrap();
    assert!(parent_events.iter().any(|e| matches!(
        e.payload(),
        Some(EventPayload::Run(RunEvent::PromotionSignaled(_)))
    )));
}

/// One observer spans the chain: the successor's events reach the same
/// surface the predecessor fed, under its own run id, so a live view draws a
/// promotion as one continuous drive.
#[tokio::test]
async fn one_observer_watches_the_whole_chain() {
    let chased = chased().await;
    let recorder = &chased.recorder;
    assert!(
        !recorder.for_run(&chased.parent).is_empty(),
        "the predecessor's own frames must be there, got kinds: {:?}",
        recorder.kinds()
    );
    assert!(
        recorder
            .for_run(&chased.last)
            .iter()
            .any(|frame| matches!(frame.payload, EventPayload::Run(RunEvent::Finished(_)))),
        "the successor's frames must reach the same observer"
    );
}
