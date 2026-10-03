//! A question asked live at the terminal that drives a run leaves
//! nothing on the log until it is answered, so the run's registry says
//! it is being asked — and stops saying so once it is not.

use std::path::PathBuf;
use std::sync::Mutex;

use yunta_core::events::{GateWaitingPayload, HumanChoice};
use yunta_engine::{Prompt, RunReport, RunTerminal};
use yunta_testkit::Bench;

mod common;
use common::*;

/// A person who, while the gate asks, notes what the run's registry says
/// about the question, then walks away.
struct Watching {
    run_dir: PathBuf,
    seen: Mutex<Vec<Option<Prompt>>>,
}

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for Watching {
    async fn resolve(&self, _escalation: &GateWaitingPayload) -> Option<HumanChoice> {
        let prompt = yunta_engine::engine_prompt(&self.run_dir, &yunta_engine::lock::SystemProbe);
        self.seen.lock().unwrap().push(prompt);
        None
    }
}

#[tokio::test]
async fn a_gate_asking_at_the_terminal_says_so_in_the_run_s_registry_until_it_stops() {
    let bench = Bench::new();
    let person = Watching {
        run_dir: bench.run_dir(),
        seen: Mutex::new(Vec::new()),
    };
    let RunReport { terminal, .. } = bench
        .run_with_interaction(INTERNAL_GATE_WORKFLOW, "sessions: []\n", &person)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let seen = person.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "{seen:?}");
    let asked = seen[0]
        .as_ref()
        .expect("the registry says the gate is asking");
    assert_eq!(
        asked.node.as_ref().map(|node| node.as_str()),
        Some("approve")
    );
    assert_eq!(
        asked.pid,
        yunta_core::Pid::try_from(std::process::id()).unwrap()
    );
    assert_eq!(
        yunta_engine::engine_prompt(&bench.run_dir(), &yunta_engine::lock::SystemProbe),
        None,
        "a run that stopped asking asks no one"
    );
}
