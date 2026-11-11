//! The questions a parked node asked, read through the API from the document it
//! handed over, for a person who answers the round from somewhere else; and a run
//! that waits on none, refused as a run that waits on no decision is.

mod bench_project;

use yunta_api::Error;
use yunta_render::refusal::Refused;
use yunta_testkit::Bench;

use bench_project::project_of;

/// A node that asks: the session hands the questions over and ends.
const ASKING: &str = "\
name: asking
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: \"Ask what has to be known before going on.\"
    artifacts:
      produces: [questions]
";

/// The session that hands one question over.
const HANDED_THEM_OVER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - { id: summary, text: "What changed?", answer_type: text, required: true }
    outcome: { type: completed, summary: asked }
"#;

/// A gate nobody answers: the run parks on a decision, not on questions.
const APPROVING: &str = "\
name: approving
nodes:
  - id: ship
    kind: gate
    assignee: the release lead
    message: \"Ship what is on the branch?\"
";

#[tokio::test]
async fn a_parked_nodes_questions_are_read_from_the_document_it_handed_over() {
    let bench = Bench::with_run_id("parked-on-questions");
    bench.run(ASKING, HANDED_THEM_OVER).await;
    let project = project_of(&bench);
    let open = project.run(&bench.run_id).await.unwrap();
    let waiting = open.waiting_questions().await.unwrap();
    assert_eq!(waiting.node.as_str(), "ask");
    let asked: Vec<&str> = waiting
        .questions
        .questions
        .iter()
        .map(|question| question.id.as_str())
        .collect();
    assert_eq!(asked, ["summary"]);
}

#[tokio::test]
async fn a_run_that_waits_on_no_questions_is_refused_as_one_that_waits_on_no_decision() {
    let bench = Bench::with_run_id("parked-on-a-gate");
    bench
        .run(APPROVING, "dialect: claude-code\nsessions: []\n")
        .await;
    let project = project_of(&bench);
    let open = project.run(&bench.run_id).await.unwrap();
    let refused = open.waiting_questions().await.expect_err("no questions");
    assert!(
        matches!(refused, Error::Refused(Refused::NoDecision { .. })),
        "{refused}"
    );
}
