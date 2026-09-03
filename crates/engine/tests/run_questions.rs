//! Runs of `kind: questions` nodes: pausing on unanswered questions and the live surface that answers them on resume.

use std::collections::HashMap;
use std::sync::Arc;

use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::AdapterId;
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, RunEnv, RunTerminal,
    DEFAULT_MAX_RETRIES,
};
use yunta_testkit::{Bench, FixedClock, MOCK_CONFIG};

mod common;
use common::*;

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
    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = questions_fixture(&artifacts_dir);

    let (terminal, _state) = bench.run(QUESTIONS_WORKFLOW, &fixture).await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(
                *reason,
                "node `ask` asked 2 question(s) awaiting an answer: q1, q2"
            );
        }
        other => panic!("expected the run to pause on unanswered questions, got {other:?}"),
    }

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::ArtifactWritten(p)) if p.path.to_string_lossy().contains("questions.yaml")
        )),
        "the questions artifact must still be recorded as written"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::NodeFinished(_))
        )),
        "a node with unanswered questions must never reach node_finished"
    );
}

#[tokio::test]
async fn resuming_a_run_paused_on_unanswered_questions_replays_the_same_pause_without_a_new_session(
) {
    // ✓ del Plan: "matar el engine durante la espera y reanudar rehace
    // las preguntas sin estado conversacional" — el segundo `execute_run`
    // usa un fixture sin sesiones disponibles; si el resume intentara
    // volver a despachar el nodo, fallaría por "fixture exhausted" en vez
    // de devolver la misma pausa.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let workflow: yunta_core::Workflow = serde_yaml::from_str(QUESTIONS_WORKFLOW).unwrap();
    let config: yunta_core::ConfigLayer = serde_yaml::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let first_adapter = MockAdapter::from_yaml(&questions_fixture(&artifacts_dir)).unwrap();
    let mut first_adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    first_adapters.insert("mock".into(), Arc::new(first_adapter));
    let first_report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &first_adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    match &first_report.terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the first run to pause, got {other:?}"),
    }

    // No `sessions:` at all — any attempt to dispatch a new session errors.
    let empty_adapter = MockAdapter::from_yaml("sessions: []").unwrap();
    let mut resume_adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    resume_adapters.insert("mock".into(), Arc::new(empty_adapter));
    let resumed_report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &resume_adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();

    assert_eq!(
        resumed_report.terminal, first_report.terminal,
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
    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = questions_fixture(&artifacts_dir);

    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "staging")], // q2 is not required
    };
    let (terminal, state) = bench
        .run_with_interaction(QUESTIONS_WORKFLOW, &fixture, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("ask"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let answered = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::QuestionsAnswered(p)) => Some(p),
            _ => None,
        })
        .expect("questions_answered must be on the log");
    assert_eq!(answered.channel, yunta_core::events::Channel::Tty);
    assert_eq!(answered.responder.as_deref(), Some("eulke"));
    assert!(!answered.answers_hash.is_empty());

    // The answers are a real artifact next to the questions, with
    // the given values, consumable by a later node via `artifact:`.
    let answers_path = artifacts_dir.join("questions.yaml.answers.yaml");
    let raw = std::fs::read_to_string(&answers_path).expect("answers artifact must exist");
    let parsed: yunta_core::AnswersFile = serde_yaml::from_str(&raw).unwrap();
    assert_eq!(parsed.answers, vec![answer("q1", "staging")]);
}

#[tokio::test]
async fn a_reply_missing_a_required_answer_pauses_citing_the_question() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = questions_fixture(&artifacts_dir);

    let interaction = ScriptedAnswers {
        answers: vec![answer("q2", "just a note")], // q1 (required) missing
    };
    let (terminal, _state) = bench
        .run_with_interaction(QUESTIONS_WORKFLOW, &fixture, &interaction)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(*reason, "node `ask` asked 1 question(s) awaiting an answer: required question `q1` has no answer");
        }
        other => panic!("an incomplete reply must pause, got {other:?}"),
    }
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::QuestionsAnswered(_))
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
    let artifacts_dir = bench.run_dir().join("artifacts");
    let workflow: yunta_core::Workflow = serde_yaml::from_str(QUESTIONS_WORKFLOW).unwrap();
    let config: yunta_core::ConfigLayer = serde_yaml::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    // First invocation: headless — asks, pauses.
    let first_adapter = MockAdapter::from_yaml(&questions_fixture(&artifacts_dir)).unwrap();
    let mut first_adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    first_adapters.insert("mock".into(), Arc::new(first_adapter));
    let first = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &first_adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    assert!(matches!(first.terminal, RunTerminal::Paused { .. }));
    // The paused node derives `waiting`, never "absent" or failed.
    assert!(
        matches!(
            first.state.nodes.get("ask"),
            Some(yunta_engine::NodeState::Waiting { .. })
        ),
        "got {:?}",
        first.state.nodes.get("ask")
    );

    // Second invocation: a live surface, an empty fixture — answering
    // needs no new session, only the log and the artifact on disk.
    let empty_adapter = MockAdapter::from_yaml("sessions: []").unwrap();
    let mut resume_adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    resume_adapters.insert("mock".into(), Arc::new(empty_adapter));
    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "production")],
    };
    let resumed = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &resume_adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &interaction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();

    assert_eq!(resumed.terminal, RunTerminal::Finished);
    assert!(matches!(
        resumed.state.nodes.get("ask"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));
    let raw = std::fs::read_to_string(artifacts_dir.join("questions.yaml.answers.yaml")).unwrap();
    let parsed: yunta_core::AnswersFile = serde_yaml::from_str(&raw).unwrap();
    assert_eq!(parsed.answers, vec![answer("q1", "production")]);
}

#[tokio::test]
async fn a_choice_answer_outside_its_declared_values_pauses_citing_the_value() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let fixture = questions_fixture(&artifacts_dir);

    let interaction = ScriptedAnswers {
        answers: vec![answer("q1", "qa")], // not in [staging, production]
    };
    let (terminal, _state) = bench
        .run_with_interaction(QUESTIONS_WORKFLOW, &fixture, &interaction)
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
