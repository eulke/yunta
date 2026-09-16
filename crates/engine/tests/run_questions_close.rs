//! How a node that asks closes: it hands its questions over, closes in full,
//! records `questions_asked`, and waits between that and `questions_answered`
//! for the `node_finished` its close deferred.

use yunta_core::events::{EventPayload, StoredEvent};
use yunta_engine::{derive, NoInteraction, NodeState, RunReport, RunTerminal};
use yunta_testkit::Bench;
use yunta_testkit_core::Log;

mod common;
use common::*;
use yunta_core::events::{ArtifactEvent, GateEvent, NodeEvent};

/// One node that asks, and the node after it that reads the answers —
/// the shape `packs/fragua` has once `grill` stops owing a brief.
const ASK_THEN_BRIEF: &str = r#"
name: ask
nodes:
  - id: grill
    kind: prompt
    runner: executor
    prompt: "Raise what you need to know."
    artifacts:
      produces: [questions]
  - id: brief
    kind: prompt
    runner: executor
    depends_on: [grill]
    context:
      - artifact: { node: grill, kind: questions }
      - artifact: { node: grill, kind: answers }
    prompt: "Write the brief from the questions and their answers."
    artifacts:
      produces: [brief.md]
"#;

/// `grill` asks one question and spends tokens doing it; `brief` writes
/// its file once the answers are in.
fn ask_then_brief_fixture(staging: &std::path::Path) -> String {
    format!(
        r##"
capabilities: {{ run_tools: true, usage_reporting: true }}
sessions:
  - steps:
      - type: usage
        input_tokens: 30
        output_tokens: 12
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - id: q1
                text: "Which environment?"
                answer_type: choice
                values: [staging, production]
                required: true
    outcome: {{ type: completed, summary: "asked" }}
  - effects:
      - {{ path: {brief:?}, content: "# Brief\n" }}
    outcome: {{ type: completed, summary: "briefed" }}
"##,
        brief = staging.join("brief.md"),
    )
}

/// The events one node wrote, as kind names in log order.
fn kinds_of(events: &[StoredEvent], node: &str) -> Vec<String> {
    events
        .iter()
        .filter(|e| e.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .map(|e| e.body.kind_name().to_string())
        .collect()
}

#[tokio::test]
async fn a_node_that_asks_records_questions_asked_and_no_terminal_event() {
    // Asking is a fact of the log, not a failure the derivation reads as a
    // wait: the node closes in full and records `questions_asked` with the
    // document it handed over and the ids awaiting an answer.
    let bench = Bench::new();
    let fixture = ask_then_brief_fixture(&bench.staging("brief"));
    let RunReport { terminal, state } = bench.run(ASK_THEN_BRIEF, &fixture).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "with no surface the run parks on its questions, got {terminal:?}"
    );
    assert_eq!(
        kinds_of(&bench.events(), "grill")
            .iter()
            .filter(|k| k.as_str() == "node_failed" || k.as_str() == "node_finished")
            .count(),
        0,
        "a node that asked has no terminal event yet: {:?}",
        kinds_of(&bench.events(), "grill")
    );
    let asked = bench
        .events()
        .iter()
        .find_map(|e| match e.payload() {
            Some(EventPayload::Gates(GateEvent::QuestionsAsked(p))) => Some(p.clone()),
            _ => None,
        })
        .expect("questions_asked must be on the log");
    assert_eq!(
        asked
            .questions
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
        ["q1"],
        "the ids awaiting an answer"
    );
    let held = bench
        .accepted()
        .into_iter()
        .find(|held| {
            held.artifact
                == yunta_core::events::ArtifactId::Interpreted {
                    kind: yunta_core::ArtifactKind::Questions,
                }
        })
        .expect("the questions the run holds");
    assert_eq!(
        asked.questions_hash, held.content_hash,
        "the fact names the document it asked from"
    );
    assert!(
        matches!(state.nodes.state("grill"), Some(NodeState::Waiting { .. })),
        "got {:?}",
        state.nodes.state("grill")
    );
}

#[tokio::test]
async fn a_node_failed_after_a_questions_artifact_derives_failed_not_waiting() {
    // The log of the production failure: a node hands over its questions and
    // then fails for an unrelated reason. The failure is a failure — a node
    // waits because it asked, never because a `questions` artifact exists.
    let bench = Bench::new();
    let events = Log::for_run(bench.run_id.as_str())
        .node(
            "grill",
            EventPayload::Node(NodeEvent::Started(
                yunta_core::events::NodeStartedPayload::attempt(1),
            )),
        )
        .node(
            "grill",
            EventPayload::Artifacts(ArtifactEvent::Accepted(
                yunta_core::events::ArtifactAcceptedPayload::new(
                    yunta_core::events::ArtifactId::Interpreted {
                        kind: yunta_core::ArtifactKind::Questions,
                    },
                    yunta_core::sha256_hex(b"questions"),
                    yunta_core::events::RecordedOrigin::Submitted,
                ),
            )),
        )
        .node(
            "grill",
            EventPayload::Node(NodeEvent::Failed(
                yunta_core::events::NodeFailedPayload::new(
                    yunta_core::events::Failure::message("scope violated: 1 file(s) outside"),
                    false,
                    Default::default(),
                ),
            )),
        )
        .build();

    let state = derive(&events);
    assert!(
        matches!(state.nodes.state("grill"), Some(NodeState::Failed { .. })),
        "a node that failed is failed, whatever artifacts it holds: got {:?}",
        state.nodes.state("grill")
    );
}

#[tokio::test]
async fn the_answer_round_finishes_the_node_without_a_second_node_started() {
    // The round records the answers and the terminal the close deferred.
    // It opens no session and starts no second attempt: the node already
    // closed, when it asked.
    let bench = Bench::new();
    let fixture = ask_then_brief_fixture(&bench.staging("brief"));
    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "staging")],
    };
    let RunReport { terminal, state } = bench
        .run_with_interaction(ASK_THEN_BRIEF, &fixture, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let grill = kinds_of(&bench.events(), "grill");
    assert_eq!(
        grill
            .iter()
            .filter(|k| k.as_str() == "node_started")
            .count(),
        1,
        "one attempt, one start: {grill:?}"
    );
    assert_eq!(
        grill
            .iter()
            .filter(|k| k.as_str() == "node_finished")
            .count(),
        1,
        "one terminal: {grill:?}"
    );
    let asked = grill.iter().position(|k| k == "questions_asked").unwrap();
    let answered = grill
        .iter()
        .position(|k| k == "questions_answered")
        .unwrap();
    let finished = grill.iter().position(|k| k == "node_finished").unwrap();
    assert!(
        asked < answered && answered < finished,
        "asked, then answered, then finished: {grill:?}"
    );
    assert!(matches!(
        state.nodes.state("grill"),
        Some(NodeState::Finished { .. })
    ));
}

#[tokio::test]
async fn a_node_that_asks_runs_its_after_hooks_and_scope_check_exactly_once() {
    // A node closes once — when it asks. The round that records the answers
    // is not a second close: no hook runs twice and no diff is audited twice.
    let bench = Bench::new();
    let workflow = r#"
name: ask
nodes:
  - id: grill
    kind: prompt
    runner: executor
    scope: ["src/**"]
    hooks:
      after:
        - run: "true"
    prompt: "Raise what you need to know."
    artifacts:
      produces: [questions]
"#;
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - id: q1
                text: "Which environment?"
                answer_type: text
                required: true
    outcome: { type: completed, summary: "asked" }
"#;
    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "staging")],
    };
    let RunReport {
        terminal,
        state: _state,
    } = bench
        .run_with_interaction(workflow, fixture, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let grill = kinds_of(&bench.events(), "grill");
    assert_eq!(
        grill
            .iter()
            .filter(|k| k.as_str() == "hook_executed")
            .count(),
        1,
        "one close, one after hook: {grill:?}"
    );
    assert_eq!(
        grill
            .iter()
            .filter(|k| k.as_str() == "scope_checked")
            .count(),
        1,
        "one close, one scope audit: {grill:?}"
    );
}

#[tokio::test]
async fn an_answered_node_owed_its_finish_is_finished_on_resume_without_a_session() {
    // A crash between the answer and the terminal leaves the node owed its
    // finish. Resume pays it from the log: no session, no second attempt.
    let bench = Bench::new();

    // First invocation: the session asks, a surface answers, and the run
    // would finish — but the process dies before `brief` runs. What is on
    // the log at that point is what the second invocation starts from.
    let fixture = ask_then_brief_fixture(&bench.staging("brief"));
    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "staging")],
    };
    bench
        .run_with_interaction(ASK_THEN_BRIEF, &fixture, &interaction)
        .await;

    // The log cut right after `questions_answered`: the node is answered
    // and owed its terminal.
    let full = bench.events();
    let cut = full
        .iter()
        .position(|e| {
            matches!(
                e.payload(),
                Some(EventPayload::Gates(GateEvent::QuestionsAnswered(_)))
            )
        })
        .expect("the answer is on the log")
        + 1;
    let state = derive(&full[..cut]);
    assert!(
        matches!(state.nodes.state("grill"), Some(NodeState::Running { .. })),
        "an answered node is running again, owed its terminal: got {:?}",
        state.nodes.state("grill")
    );
    assert!(
        state.answered_unfinished(&yunta_core::NodeId::from("grill")),
        "the log says which node owes a terminal for an answer it holds"
    );
}

#[tokio::test]
async fn the_finished_node_carries_what_the_asking_session_spent() {
    // The session that asked spent tokens and its attempt closed when it
    // asked; the terminal after the answer carries none. The run's total
    // counts that session once, not twice while it waits.
    let bench = Bench::new();
    let fixture = ask_then_brief_fixture(&bench.staging("brief"));
    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "staging")],
    };
    let RunReport {
        terminal: _terminal,
        state,
    } = bench
        .run_with_interaction(ASK_THEN_BRIEF, &fixture, &interaction)
        .await;

    let Some(NodeState::Finished { tokens, .. }) = state.nodes.state("grill") else {
        panic!("got {:?}", state.nodes.state("grill"));
    };
    assert_eq!(
        (tokens.input, tokens.output),
        (30, 12),
        "the finished node carries what the session that asked spent"
    );
    assert_eq!(
        (state.total_tokens().input, state.total_tokens().output),
        (30, 12),
        "counted once"
    );
}

#[tokio::test]
async fn a_questions_document_that_asks_nothing_leaves_empty_derived_answers_and_never_waits() {
    // A node that hands over an empty questions document asked nothing: it
    // finishes in the same close, and the answers the next node mounts are
    // there, empty, derived by the engine.
    let bench = Bench::new();
    let staging = bench.staging("brief");
    let fixture = format!(
        r##"
capabilities: {{ run_tools: true }}
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions: []
    outcome: {{ type: completed, summary: "nothing to ask" }}
  - effects:
      - {{ path: {brief:?}, content: "# Brief\n" }}
    outcome: {{ type: completed, summary: "briefed" }}
"##,
        brief = staging.join("brief.md"),
    );
    let RunReport { terminal, state } = bench.run(ASK_THEN_BRIEF, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("grill"),
        Some(NodeState::Finished { .. })
    ));
    let grill = kinds_of(&bench.events(), "grill");
    assert!(
        !grill.iter().any(|k| k == "questions_asked"),
        "nothing was asked: {grill:?}"
    );
    let answers: yunta_core::AnswersFile = serde_norway::from_slice(
        &bench
            .projection(Some("grill"), "answers.yaml")
            .expect("the answers the next node mounts"),
    )
    .unwrap();
    assert!(answers.answers.is_empty());
    let origin = bench
        .accepted()
        .into_iter()
        .find(|held| {
            held.artifact
                == yunta_core::events::ArtifactId::Interpreted {
                    kind: yunta_core::ArtifactKind::Answers,
                }
        })
        .expect("the run holds them")
        .origin;
    assert_eq!(
        origin,
        yunta_core::events::RecordedOrigin::Derived,
        "nobody answered: the engine derived them"
    );
}

#[tokio::test]
async fn the_node_that_follows_reads_the_questions_and_the_answers_of_the_node_that_asked() {
    // What a node that asks leaves behind is what the next node works
    // from: the questions and their answers, both mounted as context.
    let bench = Bench::new();
    let fixture = ask_then_brief_fixture(&bench.staging("brief"));
    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "staging")],
    };
    let RunReport { terminal, state } = bench
        .run_with_interaction(ASK_THEN_BRIEF, &fixture, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("brief"),
        Some(NodeState::Finished { .. })
    ));
    let assembled = bench
        .events()
        .iter()
        .find_map(|e| match (e.node_id.as_ref(), e.payload()) {
            (Some(id), Some(EventPayload::Node(NodeEvent::ContextAssembled(p))))
                if id.as_str() == "brief" =>
            {
                Some(p.clone())
            }
            _ => None,
        })
        .expect("`brief` assembled its context");
    let sources: Vec<&str> = assembled
        .sources
        .iter()
        .map(|s| s.source_id.as_str())
        .collect();
    assert!(
        sources.iter().any(|s| s.contains("questions"))
            && sources.iter().any(|s| s.contains("answers")),
        "both the questions and their answers reach the next node: {sources:?}"
    );
}

/// `NoInteraction` is the headless default: a run with no surface parks on
/// its questions rather than guessing an answer.
#[tokio::test]
async fn a_run_with_no_surface_parks_naming_the_questions_it_asked() {
    let bench = Bench::new();
    let fixture = ask_then_brief_fixture(&bench.staging("brief"));
    let RunReport {
        terminal,
        state: _state,
    } = bench
        .run_with_interaction(ASK_THEN_BRIEF, &fixture, &NoInteraction)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(*reason, "node `grill` asked 1 question: `q1`")
        }
        other => panic!("expected a pause on the questions, got {other:?}"),
    }
}
