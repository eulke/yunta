//! When a person begins to be asked. A run with someone at its terminal
//! says so on the log before the answer lands — the answer's events are
//! written only once it is given, the question with its answer in one
//! commit — and one with nobody there to ask says nothing of the kind.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use yunta_core::events::{EventPayload, StoredEvent};
use yunta_core::port::{Observed, RunObserver};
use yunta_core::RunId;
use yunta_engine::{RunReport, RunTerminal};
use yunta_storage::Storage;
use yunta_testkit::workflows::PROMOTABLE;
use yunta_testkit::{ApproveEverything, Bench, ScriptedInteraction};

const GATED: &str = "\
name: gated
nodes:
  - { id: approve, kind: gate, assignee: lead, options: [approve, reject] }
  - { id: after, kind: bash, run: \"true\", depends_on: [approve] }
";

/// A mock fixture with no sessions: the workflows here open none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []";

/// The gate domain's events of `events`, by kind, in the order the log has them.
fn gate_kinds(events: &[StoredEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(payload @ EventPayload::Gates(_)) => Some(payload.kind_name()),
            _ => None,
        })
        .collect()
}

/// The gate events of `run`'s log as a reader in another process finds them after each
/// commit of the run: the engine hands its observer each event inline, once the commit
/// that holds it is made, so each reading here is one that reader can make.
struct ReadAfterEachCommit {
    store: PathBuf,
    run: RunId,
    readings: Mutex<Vec<Vec<&'static str>>>,
}

impl RunObserver for ReadAfterEachCommit {
    fn observe(&self, event: Observed<'_>) {
        if *event.run_id != self.run {
            return;
        }
        let storage = Storage::open(&self.store).expect("the log opens");
        let events = storage
            .events_for_run(&self.run)
            .expect("the run's log reads");
        let mut readings = self.readings.lock().expect("the readings");
        readings.push(gate_kinds(&events));
    }
}

#[tokio::test]
async fn a_person_at_the_terminal_is_asked_on_the_log_before_the_answer() {
    let bench = Bench::new();
    let interaction = ScriptedInteraction::choose("approve");

    let RunReport { terminal, .. } = bench
        .run_with_interaction(GATED, NO_SESSIONS, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        gate_kinds(&bench.events()),
        vec!["asking_opened", "gate_waiting", "gate_resolved"]
    );
}

/// A question answered at the terminal lands on the log with its answer, in one commit:
/// read after any commit of the run, the log never holds the question without its answer,
/// so no reader elsewhere takes the run for waiting on a person who already answered. Both
/// ways a run asks at its terminal: a gate, and the decision a failed node parks on.
///
/// Reached by: `crates/engine/src/run/escalation.rs`, `crates/engine/src/run/internal_gate.rs`,
/// `crates/engine/src/run/budget.rs`, `crates/engine/src/run/console_gate.rs`,
/// `crates/engine/src/run/loop_exec/escalate.rs`
#[tokio::test]
async fn a_question_answered_at_the_terminal_lands_with_its_answer() {
    for (bench, workflow, option) in [
        (Bench::new(), GATED, "approve"),
        (Bench::new().in_mode("quick"), PROMOTABLE, "promote"),
    ] {
        let reader = Arc::new(ReadAfterEachCommit {
            store: bench.store_path().to_path_buf(),
            run: bench.run_id.clone(),
            readings: Mutex::default(),
        });
        let bench = bench.with_observer(reader.clone());
        let interaction = ScriptedInteraction::choose(option);
        bench
            .run_with_interaction(workflow, NO_SESSIONS, &interaction)
            .await;

        let readings = reader.readings.lock().expect("the readings").clone();
        let answered = readings
            .last()
            .is_some_and(|kinds| kinds.contains(&"gate_resolved"));
        assert!(answered, "the question was answered: {readings:?}");
        for kinds in &readings {
            assert_ne!(
                kinds.last(),
                Some(&"gate_waiting"),
                "a commit left the question without its answer: {readings:?}"
            );
        }
    }
}

#[tokio::test]
async fn a_run_with_nobody_to_ask_opens_no_asking() {
    let bench = Bench::new();
    let answerer = ApproveEverything::new("bot");

    let RunReport { terminal, .. } = bench
        .run_with_interaction(GATED, NO_SESSIONS, &answerer)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        gate_kinds(&bench.events()),
        vec!["gate_waiting", "gate_resolved"]
    );
}
