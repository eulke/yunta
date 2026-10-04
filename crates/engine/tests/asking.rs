//! When a person begins to be asked. A run with someone at its terminal
//! says so on the log before the answer lands — the answer's events are
//! written only once it is given — and one with nobody there to ask says
//! nothing of the kind.

use yunta_core::events::EventPayload;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{ApproveEverything, Bench, ScriptedInteraction};

const GATED: &str = "\
name: gated
nodes:
  - { id: approve, kind: gate, assignee: lead, options: [approve, reject] }
  - { id: after, kind: bash, run: \"true\", depends_on: [approve] }
";

/// The gate domain's events, by kind, in the order the log has them.
fn gate_kinds(bench: &Bench) -> Vec<&'static str> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Gates(gate)) => Some(gate.kind_name()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_person_at_the_terminal_is_asked_on_the_log_before_the_answer() {
    let bench = Bench::new();
    let interaction = ScriptedInteraction::choose("approve");

    let RunReport { terminal, .. } = bench
        .run_with_interaction(GATED, "sessions: []", &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        gate_kinds(&bench),
        vec!["asking_opened", "gate_waiting", "gate_resolved"]
    );
}

#[tokio::test]
async fn a_run_with_nobody_to_ask_opens_no_asking() {
    let bench = Bench::new();
    let answerer = ApproveEverything::new("bot");

    let RunReport { terminal, .. } = bench
        .run_with_interaction(GATED, "sessions: []", &answerer)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(gate_kinds(&bench), vec!["gate_waiting", "gate_resolved"]);
}
