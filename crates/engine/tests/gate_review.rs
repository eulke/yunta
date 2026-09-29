//! A gate that shows what it asks about, and a correction sent back to
//! the session that made the work.
//!
//! The person deciding reads the plan the gate approves — the version
//! the run holds, named on the log by its hash — and an option that sends
//! the run back to the planner asks what should change: those words are
//! what the planner's session picks its work back up with.

use yunta_core::events::{
    ArtifactId, EventPayload, GateEvent, NodeEvent, Refusal, SessionEvent, Shown,
};
use yunta_core::{ArtifactKind, TasksFile};
use yunta_engine::{current_escalation, RunError, RunReport, RunTerminal, ShownContent};
use yunta_testkit::{Bench, MOCK_CONFIG};

mod common;
use common::*;

/// A planner, and the gate that shows its plan and may send it back.
const REVIEWED_PLAN: &str = r#"
name: reviewed-plan
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    message: "Plan registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows: [{ node: plan, kind: tasks }]
"#;

/// Two planner sessions: the first plan, and the one after a correction.
fn planner(resume: bool) -> String {
    format!(
        r#"
capabilities: {{ run_tools: true, resume_session: {resume} }}
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Make it"
            description: "Writes the file the run is about."
            tasks:
              - {{ id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], criteria: [{{ cmd: "test -f made.txt", proves: "the file exists" }}] }}
    outcome: {{ type: completed, summary: "planned" }}
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Make it"
            description: "Writes the file the run is about."
            tasks:
              - {{ id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], criteria: [{{ cmd: "test -f made.txt", proves: "the file exists" }}] }}
              - {{ id: T002, title: "Say it", description: "Writes said.txt.", scope: [said.txt], criteria: [{{ cmd: "test -f said.txt", proves: "it was said" }}] }}
    outcome: {{ type: completed, summary: "planned again" }}
"#
    )
}

const SAID: &str = "split the greeting into its own task";

/// Every `gate_waiting` the run recorded, oldest first.
fn waiting(bench: &Bench) -> Vec<yunta_core::events::GateWaitingPayload> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::Waiting(waiting))) => Some(waiting.clone()),
            _ => None,
        })
        .collect()
}

/// Every session the run opened, oldest first.
fn opened(bench: &Bench) -> Vec<yunta_core::events::AgentSessionOpenedPayload> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Session(SessionEvent::Opened(opened))) => Some(opened.clone()),
            _ => None,
        })
        .collect()
}

/// The plan the run holds now, as the log names it.
fn held_plan(bench: &Bench) -> Shown {
    let tasks = ArtifactId::Interpreted {
        kind: ArtifactKind::Tasks,
    };
    let state = yunta_engine::derive(&bench.events());
    let held = state
        .artifacts
        .latest(&tasks, Some(&"plan".into()))
        .expect("the plan is held");
    Shown {
        producer: held.producer.clone(),
        artifact: held.artifact.clone(),
        content_hash: held.content_hash.clone(),
    }
}

#[tokio::test]
async fn the_gate_shows_the_plan_it_asks_about_as_the_run_holds_it() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["approve"]);
    let RunReport { terminal, .. } = bench
        .run_with_interaction(REVIEWED_PLAN, &planner(true), &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        waiting(&bench)[0].shows(),
        [held_plan(&bench)],
        "the decision is recorded against the bytes it was about"
    );
    let shown = interaction.shown();
    let ShownContent::Tasks(TasksFile { tasks, .. }) = &shown[0][0].content else {
        panic!("a tasks document is shown as its tasks: {shown:?}");
    };
    let ids: Vec<&str> = tasks.iter().map(|task| task.id.as_str()).collect();
    assert_eq!(ids, ["T001"]);
    assert!(
        shown[0][0].path.ends_with("artifacts/plan/tasks.md"),
        "a plan points to the view written for the person reviewing it"
    );
}

#[tokio::test]
async fn an_option_that_sends_the_plan_back_asks_what_should_change() {
    let bench = parked(REVIEWED_PLAN, &planner(true)).await;

    let (_, escalation) =
        current_escalation(&bench.manifest(), &yunta_engine::derive(&bench.events()))
            .expect("the run waits on the gate");
    let asks: Vec<(&str, Option<&str>)> = escalation
        .options()
        .iter()
        .map(|option| (option.id.as_str(), option.asks.as_deref()))
        .collect();
    assert_eq!(
        asks,
        [
            ("approve", None),
            ("adjust", Some("what should change?")),
            ("abort", None)
        ]
    );
    assert_eq!(
        escalation.shows(),
        [held_plan(&bench)],
        "a parked run rebuilds what the gate shows from the log"
    );
}

#[tokio::test]
async fn a_correction_without_words_is_refused_wherever_it_comes_from() {
    let bench = parked(REVIEWED_PLAN, &planner(true)).await;
    let refused = answer_parked(&bench, "adjust").await;
    assert!(
        matches!(refused, Err(yunta_engine::ResolveGateError::Unsaid { .. })),
        "{refused:?}"
    );

    let bench = Bench::new();
    bench
        .create(REVIEWED_PLAN, &planner(true), MOCK_CONFIG)
        .await;
    let error = bench
        .try_wake_answering(&SequencedInteraction::choosing(&["adjust"]))
        .await
        .expect_err("a correction that says nothing is not a decision");
    assert!(
        matches!(
            error,
            RunError::RefusedAnswer {
                refused: Refusal::Unsaid { .. },
                ..
            }
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_correction_resumes_the_planner_with_the_persons_words() {
    let bench = Bench::new();
    let interaction =
        SequencedInteraction::answering(vec![("adjust", Some(SAID)), ("approve", None)]);
    let RunReport { terminal, .. } = bench
        .run_with_interaction(REVIEWED_PLAN, &planner(true), &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let opened = opened(&bench);
    assert_eq!(opened.len(), 2, "{opened:?}");
    assert_eq!(opened[1].continues.as_ref(), Some(&opened[0].session_id));
    let resumed = bench.mock().requests_seen()[1].clone();
    assert!(
        resumed.prompt.contains(SAID)
            && resumed.prompt.contains("approve-plan")
            && !resumed.prompt.contains("Hand over the tasks document."),
        "the session is told the review, not the brief again: {}",
        resumed.prompt
    );
    let rerouted = bench.events().iter().any(|event| {
        matches!(
            event.payload(),
            Some(EventPayload::Node(NodeEvent::Rerouted(p))) if p.cause.to_string().contains(SAID)
        )
    });
    assert!(rerouted, "the re-route says why the plan went back");
    let waiting = waiting(&bench);
    assert_ne!(
        waiting[0].shows(),
        waiting[1].shows(),
        "the second question is about the second plan"
    );
}

#[tokio::test]
async fn without_a_resumable_session_a_fresh_planner_reads_the_review_and_its_plan() {
    let bench = Bench::new();
    let interaction =
        SequencedInteraction::answering(vec![("adjust", Some(SAID)), ("approve", None)]);
    let RunReport { terminal, .. } = bench
        .run_with_interaction(REVIEWED_PLAN, &planner(false), &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let opened = opened(&bench);
    assert_eq!(opened[1].continues, None);
    let fresh = bench.mock().requests_seen()[1].clone();
    assert!(
        fresh.prompt.contains("Hand over the tasks document.")
            && fresh.prompt.contains(SAID)
            && fresh.prompt.contains("artifacts/plan/tasks.yaml"),
        "a fresh session gets the brief, the review and where its plan is: {}",
        fresh.prompt
    );
    let degraded = bench.events().iter().any(|event| {
        matches!(
            event.payload(),
            Some(EventPayload::Session(SessionEvent::CapabilityDegraded(_)))
        )
    });
    assert!(degraded, "the log says the session was not picked back up");
}

/// The same planner and gate under two modes of the workflow's own
/// naming: one runs the gate that shows the plan, the other leaves it out.
fn two_ways() -> String {
    REVIEWED_PLAN.replace(
        "name: reviewed-plan\n",
        "name: reviewed-plan\nmodes:\n  pepito: { include: [plan, approve-plan] }\n  \
         pirulo: { include: [plan] }\n",
    )
}

/// A plan with nothing for a person to read, then — when `refused` —
/// the same plan explained, after the engine refuses the first.
fn unexplained_then_explained(refused: bool) -> String {
    let unexplained = r#"
      - type: run_tool
        tool: yunta_submit_tasks
        EXPECT
        arguments:
          document:
            tasks:
              - { id: T001, title: "Make it", scope: [made.txt], criteria: [{ cmd: "test -f made.txt" }] }"#;
    let explained = r#"
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Make it"
            description: "Writes the file the run is about."
            tasks:
              - { id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], criteria: [{ cmd: "test -f made.txt", proves: "the file exists" }] }"#;
    let steps = match refused {
        true => format!(
            "{}{explained}",
            unexplained.replace("EXPECT", "expect: refused")
        ),
        false => unexplained.replace("        EXPECT\n", ""),
    };
    format!(
        "\ncapabilities: {{ run_tools: true }}\nsessions:\n  - steps:{steps}\n    outcome: \
         {{ type: completed, summary: \"planned\" }}\n"
    )
}

/// What the engine said when it refused a submission, as the log
/// carries it.
fn refusals(bench: &Bench) -> Vec<String> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Artifacts(yunta_core::events::ArtifactEvent::Submitted(p))) => {
                match &p.outcome {
                    yunta_core::events::SubmissionOutcome::Refused { report } => {
                        Some(serde_json::to_string(report).unwrap())
                    }
                    yunta_core::events::SubmissionOutcome::Accepted { .. } => None,
                }
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_plan_a_gate_will_show_is_refused_until_it_explains_itself() {
    let bench = Bench::new().in_mode("pepito");
    let RunReport { terminal, .. } = bench
        .run(&two_ways(), &unexplained_then_explained(true))
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let refusals = refusals(&bench);
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    for code in ["no-summary", "no-description", "unexplained-criterion"] {
        assert!(
            refusals[0].contains(code),
            "the refusal names `{code}`: {}",
            refusals[0]
        );
    }
}

#[tokio::test]
async fn a_plan_no_gate_of_the_run_shows_needs_no_explanation() {
    let bench = Bench::new().in_mode("pirulo");
    let RunReport { terminal, .. } = bench
        .run(&two_ways(), &unexplained_then_explained(false))
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(refusals(&bench).is_empty());
}

/// A plan with everything a person reviewing it reads: a description
/// with a diagram, a design, a risk, and two tasks one after the other.
const EXPLAINED: &str = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Greet and say it"
            description: |
              Two files, one after the other.

              ```mermaid
              graph LR
                made --> said
              ```
            design: |
              ```text
              made.txt: one line
              ```
            risks: ["The files already exist"]
            tasks:
              - { id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], criteria: [{ cmd: "test -f made.txt", proves: "the file exists" }, { cmd: "true", type: guard, proves: "nothing else breaks" }] }
              - { id: T002, title: "Say it", description: "Writes said.txt.", depends_on: [T001], scope: [said.txt], criteria: [{ cmd: "test -f said.txt", proves: "it was said" }] }
    outcome: { type: completed, summary: "planned" }
"#;

/// The view the engine writes for [`EXPLAINED`], byte for byte.
const EXPLAINED_VIEW: &str = "# Greet and say it

Two files, one after the other.

```mermaid
graph LR
  made --> said
```

## Design

```text
made.txt: one line
```

## At a glance

- **Tasks:** 2
- **Order:** T001; then T002
- **Touches:** `made.txt`, `said.txt`
- **Must keep passing:** `true`

```mermaid
graph LR
  T001[\"T001: Make it\"]
  T002[\"T002: Say it\"]
  T001 --> T002
```

## Risks

- The files already exist

## Tasks

### T001 — Make it

Writes made.txt.

**Touches:** `made.txt`

| Done when | Command |
|---|---|
| the file exists | `test -f made.txt` |
| keeps passing: nothing else breaks | `true` |

### T002 — Say it

Writes said.txt.

**Touches:** `said.txt`

| Done when | Command |
|---|---|
| it was said | `test -f said.txt` |

**After:** T001
";

#[tokio::test]
async fn the_plan_is_also_written_for_the_person_who_reviews_it() {
    let bench = Bench::new().in_mode("pirulo");
    bench.run(&two_ways(), EXPLAINED).await;

    let view = String::from_utf8(bench.projection(Some("plan"), "tasks.md").unwrap()).unwrap();
    assert_eq!(view, EXPLAINED_VIEW);
}
