//! A plan read beside what the person deciding on it already answered: a
//! decision that restates an answer is theirs, not one the plan made for
//! them, and the gate shows the question and the answer as the run holds
//! them.

use std::sync::Mutex;

use yunta_core::events::{GateWaitingPayload, HumanChoice};
use yunta_core::shown::Answered;
use yunta_engine::{RunReport, RunTerminal, ShownContent, ShownDocument};
use yunta_testkit::Bench;

mod common;
use common::*;

/// A grill that asks, a planner that reads its answers, and a gate that
/// shows the plan.
const ASKED_THEN_PLANNED: &str = r#"
name: asked-then-planned
nodes:
  - id: grill
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Raise what you need to know."
    artifacts:
      produces: [questions]
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    depends_on: [grill]
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Plan registered. Approve?"
    shows: [{ node: plan, kind: tasks }]
"#;

/// The grill's one question, and a plan whose decision restates its
/// answer.
const SESSIONS: &str = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - { id: environment, text: "Which environment does it deploy to?", answer_type: choice, values: [staging, production], required: true }
    outcome: { type: completed, summary: "asked" }
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Deploy it"
            description: "Writes where the run deploys."
            decisions:
              - { id: target, question: "Where does it deploy?", choice: "To staging.", why: "The person said so.", answers: environment }
            tasks:
              - { id: T001, title: "Name the target", description: "Writes target.txt.", scope: [target.txt], changes: [{ at: target.txt, what: "the target", code: "staging" }], outcome: "target.txt names staging", criteria: [{ cmd: "test \"$(cat target.txt 2>/dev/null)\" = staging", proves: "the target is staging" }] }
    outcome: { type: completed, summary: "planned" }
"#;

/// A person who answers the grill with `staging`, then reads what the
/// gate shows and walks away.
#[derive(Default)]
struct Answering(Mutex<Vec<Vec<ShownDocument>>>);

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for Answering {
    async fn resolve(&self, _escalation: &GateWaitingPayload) -> Option<HumanChoice> {
        None
    }

    async fn resolve_in(
        &self,
        _escalation: &GateWaitingPayload,
        asking: &yunta_engine::Asking<'_>,
    ) -> Option<HumanChoice> {
        self.0.lock().unwrap().push(asking.shown.to_vec());
        None
    }

    async fn ask(
        &self,
        _node: &yunta_core::NodeId,
        _questions: &yunta_core::QuestionsFile,
    ) -> Option<yunta_engine::QuestionsReply> {
        Some(yunta_engine::QuestionsReply {
            answers: vec![answer("environment", "staging")],
            channel: yunta_core::events::Channel::Tty,
            responder: Some("lead".into()),
        })
    }
}

#[tokio::test]
async fn a_shown_plan_carries_the_questions_the_run_asked_with_their_answers() {
    let bench = Bench::new();
    let person = Answering::default();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(ASKED_THEN_PLANNED, SESSIONS, &person)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let shown = person.0.lock().unwrap().clone();
    let ShownContent::Tasks(review) = &shown[0][0].content else {
        panic!("the gate shows the plan: {shown:?}");
    };
    assert_eq!(
        review.answered,
        [Answered {
            id: "environment".into(),
            question: "Which environment does it deploy to?".to_string(),
            answer: "staging".to_string(),
        }]
    );
}
