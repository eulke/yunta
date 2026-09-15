//! Runs of `kind: questions` nodes: pausing on unanswered questions and the live surface that answers them on resume.

use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

mod common;
use common::*;
use yunta_core::events::{GateEvent, NodeEvent};

#[tokio::test]
async fn a_questions_artifact_pauses_the_run_after_its_own_session_already_closed() {
    // "el nodo que pregunta cierra su sesión antes de que se
    // renderice nada" (la sesión mock corre y cierra normalmente, y solo
    // *después* de eso el engine actúa sobre las preguntas) y "sin TTY el
    // run queda `waiting`, nunca cuelga ni falla" — en este recorte no
    // existe ninguna superficie TTY/MCP/PR todavía, así que ese
    // es el único camino: el run pausa citando las preguntas, no panickea
    // ni queda colgado.
    let bench = Bench::new();

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(QUESTIONS_WORKFLOW, QUESTIONS_FIXTURE).await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(
                *reason,
                "node `ask` asked 2 question(s) awaiting an answer: `q1`, `q2`"
            );
        }
        other => panic!("expected the run to pause on unanswered questions, got {other:?}"),
    }

    let events = bench.events();
    assert!(
        bench.accepted().iter().any(|held| held.artifact
            == yunta_core::events::ArtifactId::Interpreted {
                kind: yunta_core::ArtifactKind::Questions
            }),
        "the questions artifact must still be an artifact the run holds"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::Node(NodeEvent::Finished(
                _
            )))
        )),
        "a node with unanswered questions must never reach node_finished"
    );
}

#[tokio::test]
async fn resuming_a_run_paused_on_unanswered_questions_replays_the_same_pause_without_a_new_session(
) {
    // ✓ del Plan: "matar el engine durante la espera y reanudar rehace
    // las preguntas sin estado conversacional" — el segundo despacho
    // usa un fixture sin sesiones disponibles; si el resume intentara
    // volver a despachar el nodo, fallaría por "fixture exhausted" en vez
    // de devolver la misma pausa.
    let bench = Bench::new();

    let RunReport {
        terminal: first_terminal,
        ..
    } = bench.run(QUESTIONS_WORKFLOW, QUESTIONS_FIXTURE).await;
    match &first_terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the first run to pause, got {other:?}"),
    }

    // No `sessions:` at all — any attempt to dispatch a new session errors.
    let RunReport {
        terminal: resumed_terminal,
        ..
    } = bench.wake_on_fixture("sessions: []").await;

    assert_eq!(
        resumed_terminal, first_terminal,
        "resume must replay the exact same pause, no new session needed"
    );
}

#[tokio::test]
async fn answered_questions_finish_the_node_and_materialize_the_answers_artifact() {
    // With a live surface, the questions are answered in the same
    // invocation — the node finishes, the answers land as an artifact a
    // following node can mount, and `questions_answered` records hash,
    // channel and responder.
    let bench = Bench::new();

    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "staging")], // q2 is not required
    };
    let RunReport { terminal, state } = bench
        .run_with_interaction(QUESTIONS_WORKFLOW, QUESTIONS_FIXTURE, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("ask"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));

    let events = bench.events();
    let answered = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Gates(GateEvent::QuestionsAnswered(p))) => {
                Some(p)
            }
            _ => None,
        })
        .expect("questions_answered must be on the log");
    assert_eq!(answered.channel, yunta_core::events::Channel::Tty);
    assert_eq!(
        answered.responder.as_ref().map(|r| r.as_str()),
        Some("eulke")
    );

    // The answers are a real artifact next to the questions, with
    // the given values, consumable by a later node via `artifact:`.
    let raw = String::from_utf8(
        bench
            .projection(Some("ask"), "answers.yaml")
            .expect("the answers artifact has a view"),
    )
    .unwrap();
    let parsed: yunta_core::AnswersFile = serde_norway::from_str(&raw).unwrap();
    assert_eq!(parsed.answers, vec![answer("q1", "staging")]);

    // The answers are the run's too, with the origin that says the
    // engine materialized them from what a person replied.
    let held = bench
        .accepted()
        .into_iter()
        .find(|held| {
            held.artifact
                == yunta_core::events::ArtifactId::Interpreted {
                    kind: yunta_core::ArtifactKind::Answers,
                }
        })
        .expect("the answers are an artifact the run holds");
    assert_eq!(held.origin, yunta_core::events::RecordedOrigin::Answered);
    assert_eq!(held.content_hash, answered.answers_hash);
    assert_eq!(
        bench
            .object(&held.content_hash)
            .expect("the bytes are in the store"),
        raw.as_bytes()
    );
}

/// A surface that answered and a surface that could not are two
/// different facts, and the pause says which.
#[tokio::test]
async fn a_reply_missing_a_required_answer_pauses_citing_the_question() {
    let bench = Bench::new();

    let interaction = ScriptedAnswers {
        answers: vec![answer("q2", "just a note")], // q1 (required) missing
    };
    let RunReport {
        terminal,
        state: _state,
    } = bench
        .run_with_interaction(QUESTIONS_WORKFLOW, QUESTIONS_FIXTURE, &interaction)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert!(
                reason.starts_with("node `ask`'s answers were refused:"),
                "the pause says whose answers and that they were refused: {reason}"
            );
            assert!(
                reason.contains("`q1`") && reason.contains("nothing answers it"),
                "and which question went unanswered, in the engine's own words: {reason}"
            );
        }
        other => panic!("an incomplete reply must pause, got {other:?}"),
    }
    let events = bench.events();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::Gates(
                GateEvent::QuestionsAnswered(_)
            ))
        )),
        "an invalid reply must never be recorded as answered"
    );
}

#[tokio::test]
async fn resuming_a_questions_pause_with_a_live_surface_answers_and_continues() {
    // The waiting state is derived from the log, so a *separate*
    // invocation (yunta resume with a TTY) re-asks and continues — no
    // conversational state, no new agent session.
    let bench = Bench::new();

    // First invocation: headless — asks, pauses.
    let RunReport {
        terminal: first_terminal,
        state: first_state,
    } = bench.run(QUESTIONS_WORKFLOW, QUESTIONS_FIXTURE).await;
    assert!(matches!(first_terminal, RunTerminal::Paused { .. }));
    // The paused node derives `waiting`, never "absent" or failed.
    assert!(
        matches!(
            first_state.nodes.state("ask"),
            Some(yunta_engine::NodeState::Waiting { .. })
        ),
        "got {:?}",
        first_state.nodes.state("ask")
    );

    // Second invocation: a live surface, an empty fixture — answering
    // needs no new session, only the log and the artifact on disk.
    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "production")],
    };
    let RunReport {
        terminal: resumed_terminal,
        state: resumed_state,
    } = bench
        .wake_on_fixture_answering("sessions: []", &interaction)
        .await;

    assert_eq!(resumed_terminal, RunTerminal::Finished);
    assert!(matches!(
        resumed_state.nodes.state("ask"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));
    let raw = String::from_utf8(bench.projection(Some("ask"), "answers.yaml").unwrap()).unwrap();
    let parsed: yunta_core::AnswersFile = serde_norway::from_str(&raw).unwrap();
    assert_eq!(parsed.answers, vec![answer("q1", "production")]);
}

#[tokio::test]
async fn a_choice_answer_outside_its_declared_values_pauses_citing_the_value() {
    let bench = Bench::new();

    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "qa")], // not in [staging, production]
    };
    let RunReport {
        terminal,
        state: _state,
    } = bench
        .run_with_interaction(QUESTIONS_WORKFLOW, QUESTIONS_FIXTURE, &interaction)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert!(
                reason.contains("qa") || reason.contains("q1"),
                "must cite the invalid value or its question: {reason}"
            );
        }
        other => panic!("an out-of-values choice must pause, got {other:?}"),
    }
}
