//! A question asked live at the terminal that drives a run leaves
//! nothing on the log until it is answered, so the run's registry says
//! it is being asked — and stops saying so once it is not.

use std::path::PathBuf;
use std::sync::Mutex;

use yunta_core::events::{GateWaitingPayload, HumanChoice};
use yunta_engine::{RunReport, RunTerminal};
use yunta_replay::registry::Engine;
use yunta_testkit::Bench;

mod common;
use common::*;

/// A person who, while the gate asks, notes what the run's registry says
/// about the question, then walks away.
struct Watching {
    run_dir: PathBuf,
    seen: Mutex<Vec<Engine>>,
}

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for Watching {
    async fn resolve(&self, _escalation: &GateWaitingPayload) -> Option<HumanChoice> {
        let engine = yunta_engine::read_engine(&self.run_dir, &yunta_engine::lock::SystemProbe);
        self.seen.lock().unwrap().push(engine);
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
        .run_with_interaction(
            INTERNAL_GATE_WORKFLOW,
            "dialect: claude-code\nsessions: []\n",
            &person,
        )
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let seen = person.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "{seen:?}");
    let Engine::Alive {
        pid,
        prompt: Some(asked),
        ..
    } = &seen[0]
    else {
        panic!("the registry says the gate is asking: {seen:?}");
    };
    assert_eq!(
        asked.node.as_ref().map(|node| node.as_str()),
        Some("approve")
    );
    assert_eq!(
        *pid,
        yunta_core::Pid::current(),
        "the process asking is the engine"
    );
    assert_eq!(
        yunta_engine::read_engine(&bench.run_dir(), &yunta_engine::lock::SystemProbe).prompt(),
        None,
        "a run that stopped asking asks no one"
    );
}
