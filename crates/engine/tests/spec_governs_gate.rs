//! A gate never offers to go on with a plan that cannot be proven as it
//! is written.
//!
//! The plan here is accepted when it is handed over — the spec that
//! makes it unprovable comes after it — and only the gate sees both: the
//! plan changes the test its spec then writes, which no session may
//! write. A gate answered here withholds going on and says why; one on a
//! forge publishes nothing and sends the plan back.

use std::sync::{Arc, Mutex};

use yunta_adapters::{MockForge, MockForgeState};
use yunta_core::events::{
    EventDraft, EventPayload, FindingEvent, FindingSeverity, GateEvent, GateResolvedPayload,
    GateWaitingPayload, HumanChoice, NodeEvent, SessionEvent,
};
use yunta_engine::{current_escalation, ResolveGateError, RunReport, RunTerminal};
use yunta_testkit::Bench;
use yunta_testkit_core::FixedClock;

mod common;
use common::spec::{spec_of, specifying, GREETS};
use common::*;

/// The planner, the node that specifies its tasks, and a gate that shows
/// both and may send the plan back.
const GOVERNED: &str = r#"
name: governed
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: spec
    kind: prompt
    runner: planner
    permissions: read-only
    depends_on: [plan]
    prompt: "Write the tests the plan's tasks are held to."
    artifacts:
      produces: [spec]
  - id: approve-plan
    kind: gate
    depends_on: [spec]
    assignee: lead
    message: "Plan and its tests registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows:
      - { node: plan, kind: tasks }
      - { node: spec, kind: spec }
"#;

/// [`GOVERNED`] with the gate on a forge, sending a request for changes
/// back to the planner.
const GOVERNED_ON_A_FORGE: &str = r#"
name: governed
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: spec
    kind: prompt
    runner: planner
    permissions: read-only
    depends_on: [plan]
    prompt: "Write the tests the plan's tasks are held to."
    artifacts:
      produces: [spec]
  - id: approve-plan
    kind: gate
    depends_on: [spec]
    assignee: lead
    message: "Is this plan the one to build?"
    on_failure: { goto: plan, max_reroutes: 1 }
    external:
      kind: pull_request
      artifacts: [tasks, spec]
      branch: "{{run.branch}}"
"#;

/// A plan whose task changes `tests/greet.sh` — the file its spec then
/// writes as the task's test.
const CHANGES_ITS_TEST: &str = r#"summary: "Greet"
description: "Writes the greeting the run is about."
tasks:
  - id: greet
    title: "Write the greeting"
    description: "Writes greeting.txt, and a check of it."
    scope: [greeting.txt, tests/greet.sh]
    changes:
      - { at: greeting.txt, what: "the greeting", code: "Hello" }
      - { at: tests/greet.sh, what: "a check of the greeting", code: "test -s greeting.txt" }
    outcome: "greeting.txt says hello"
    criteria:
      - { cmd: "test \"$(cat greeting.txt 2>/dev/null)\" = Hello", proves: "the greeting says hello" }
"#;

/// [`CHANGES_ITS_TEST`], leaving the test to the spec.
const LEAVES_ITS_TEST: &str = r#"summary: "Greet"
description: "Writes the greeting the run is about."
tasks:
  - id: greet
    title: "Write the greeting"
    description: "Writes greeting.txt."
    scope: [greeting.txt, tests/greet.sh]
    changes:
      - { at: greeting.txt, what: "the greeting", code: "Hello" }
    outcome: "greeting.txt says hello"
    criteria:
      - { cmd: "test \"$(cat greeting.txt 2>/dev/null)\" = Hello", proves: "the greeting says hello" }
"#;

/// The spec of `greet`: `tests/greet.sh`, run by its one test.
fn spec_session() -> String {
    let spec = spec_of("greet", GREETS, "sh tests/greet.sh");
    specifying(&[(&spec, true)])
}

/// The planner handing over each plan in turn, and the spec after each.
fn planning(plans: &[&str]) -> String {
    let sessions: String = plans
        .iter()
        .map(|plan| {
            tasks_session(plan, "planned").replacen(
                "  - steps:",
                "  - match_prompt_contains: \"Hand over the tasks\"\n    steps:",
                1,
            ) + &spec_session()
        })
        .collect();
    format!("capabilities: {{ run_tools: true }}\nsessions:\n{sessions}")
}

/// A person at the run's terminal who reads each escalation and walks
/// away: the run parks on it.
#[derive(Default)]
struct Reading(Mutex<Vec<GateWaitingPayload>>);

impl Reading {
    fn asked(&self) -> Vec<GateWaitingPayload> {
        self.0.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for Reading {
    async fn resolve(&self, escalation: &GateWaitingPayload) -> Option<HumanChoice> {
        self.0.lock().unwrap().push(escalation.clone());
        None
    }
}

/// The ids of `escalation`'s menu, in order.
fn menu(escalation: &GateWaitingPayload) -> Vec<String> {
    escalation
        .options()
        .iter()
        .map(|option| option.id.to_string())
        .collect()
}

/// What the run recorded for `node`, in order.
fn of_node<'a>(
    events: &'a [yunta_core::events::StoredEvent],
    node: &'a str,
) -> impl Iterator<Item = &'a EventPayload> {
    events
        .iter()
        .filter(move |event| event.node_id.as_ref().map(|id| id.as_str()) == Some(node))
        .filter_map(|event| event.payload())
}

/// Every finding `node` posted.
fn posted(
    events: &[yunta_core::events::StoredEvent],
    node: &str,
) -> Vec<yunta_core::events::Finding> {
    of_node(events, node)
        .filter_map(|payload| match payload {
            EventPayload::Findings(FindingEvent::Posted(posted)) => Some(posted.finding.clone()),
            _ => None,
        })
        .collect()
}

/// What the gate a parked run waits on asks, rebuilt from its log.
async fn rebuilt(bench: &Bench) -> GateWaitingPayload {
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
    .expect("the run waits on the gate");
    escalation.into_payload()
}

#[tokio::test]
async fn a_gate_withholds_going_on_with_a_plan_that_changes_its_own_spec_test() {
    let bench = Bench::new();
    let person = Reading::default();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(GOVERNED, &planning(&[CHANGES_ITS_TEST]), &person)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let asked = person.asked();
    assert_eq!(asked.len(), 1, "the gate asked once: {asked:?}");
    assert_eq!(menu(&asked[0]), ["adjust", "abort"]);
    let withheld = asked[0].withheld();
    assert_eq!(withheld.len(), 1, "{withheld:?}");
    assert_eq!(withheld[0].option.as_str(), "approve");
    assert!(
        withheld[0].because.contains("`tests/greet.sh`"),
        "the reason names what keeps the plan from being proven: {}",
        withheld[0].because
    );
    assert!(
        asked[0]
            .evidence()
            .lines()
            .iter()
            .any(|line| line.starts_with("withheld: `approve`")),
        "a surface that prints only the record still says why: {:?}",
        asked[0].evidence().lines()
    );
    assert_eq!(
        rebuilt(&bench).await,
        asked[0],
        "a parked run rebuilds the menu it was asked with"
    );
}

#[tokio::test]
async fn a_withheld_option_is_refused_with_why_when_it_is_answered_from_elsewhere() {
    let bench = parked(GOVERNED, &planning(&[CHANGES_ITS_TEST])).await;

    let refused = answer_parked(&bench, "approve").await;

    let Err(ResolveGateError::Withheld { chosen, because }) = refused else {
        panic!("approving a plan that cannot be proven is refused: {refused:?}");
    };
    assert_eq!(chosen.as_str(), "approve");
    assert!(because.contains("`tests/greet.sh`"), "{because}");
}

#[tokio::test]
async fn an_approval_seeded_before_the_plan_was_found_unprovable_is_asked_again() {
    let bench = parked(GOVERNED, &planning(&[CHANGES_ITS_TEST])).await;
    // What a binary that did not withhold would have recorded for a
    // person who approved while the run was parked.
    let waiting = rebuilt(&bench).await;
    for payload in [
        GateEvent::Waiting(waiting),
        GateEvent::Resolved(GateResolvedPayload::Chosen(HumanChoice {
            option: "approve".into(),
            by: "mcp".into(),
            free_text: None,
        })),
    ] {
        bench
            .storage
            .append(
                &EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some("approve-plan".into()),
                    payload: EventPayload::Gates(payload),
                },
                &FixedClock,
            )
            .unwrap();
    }

    let person = Reading::default();
    let RunReport { terminal, state } = bench.wake_answering(&person).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        person.asked().len(),
        1,
        "the approval no longer stands, so the gate asks"
    );
    assert!(
        !matches!(
            state.nodes.state("approve-plan"),
            Some(yunta_engine::NodeState::Finished { .. })
        ),
        "the plan is not approved: {:?}",
        state.nodes.state("approve-plan")
    );
}

#[tokio::test]
async fn a_forge_gate_sends_an_unprovable_plan_back_without_publishing_it() {
    let forge = MockForgeState::new();
    let bench = Bench::new().with_forge(Arc::new(MockForge::new(forge.clone())));

    let RunReport { terminal, .. } = bench
        .run(
            GOVERNED_ON_A_FORGE,
            &planning(&[CHANGES_ITS_TEST, LEAVES_ITS_TEST]),
        )
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let events = bench.events();
    let flaws = posted(&events, "approve-plan");
    assert_eq!(flaws.len(), 1, "one finding per flaw: {flaws:?}");
    assert_eq!(flaws[0].severity, FindingSeverity::Blocking);
    assert!(flaws[0].title.contains("`tests/greet.sh`"), "{flaws:?}");
    assert!(
        flaws[0]
            .location
            .path()
            .ends_with("artifacts/plan/tasks.yaml"),
        "the finding is on the plan the planner rewrites: {flaws:?}"
    );
    let planners = of_node(&events, "plan")
        .filter(|payload| matches!(payload, EventPayload::Session(SessionEvent::Opened(_))))
        .count();
    assert_eq!(planners, 2, "the plan went back to its planner");
    let gate: Vec<_> = of_node(&events, "approve-plan").collect();
    let sent_back = gate
        .iter()
        .position(|payload| matches!(payload, EventPayload::Node(NodeEvent::Failed(_))));
    let published = gate
        .iter()
        .position(|payload| matches!(payload, EventPayload::Gates(GateEvent::Waiting(_))));
    assert!(
        matches!((sent_back, published), (Some(back), Some(at)) if back < at),
        "nothing was published before the plan came back: {sent_back:?} {published:?}"
    );
    let prs = forge.pull_requests();
    assert_eq!(prs.len(), 1, "only the corrected plan is published");
    let tasks = String::from_utf8(prs[0].files["tasks.yaml"].clone()).unwrap();
    assert!(!tasks.contains("a check of the greeting"), "{tasks}");
}

#[tokio::test]
async fn a_gate_with_no_forge_withholds_approving_an_unprovable_plan_at_the_console() {
    let bench = Bench::new();
    let person = Reading::default();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(GOVERNED_ON_A_FORGE, &planning(&[CHANGES_ITS_TEST]), &person)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let asked = person.asked();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(menu(&asked[0]), ["reject"]);
    assert_eq!(asked[0].withheld()[0].option.as_str(), "approve");
}
