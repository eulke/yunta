//! Sessions end-to-end: session events, the skills chain, on_finish distillation, runner fan-out, forensic events.jsonl, and resume_session.

use std::collections::HashMap;
use std::sync::Arc;

use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::{AdapterId, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_testkit::{Bench, FixedClock, MOCK_CONFIG};

mod common;
use common::*;

#[tokio::test]
async fn a_session_leaves_agent_session_opened_in_the_log_with_its_session_id() {
    let bench = Bench::new();
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _) = bench.run(SESSION_EVENTS_WORKFLOW, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let opened = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::AgentSessionOpened(p)) => {
                Some((e.node_id.clone(), p.clone()))
            }
            _ => None,
        })
        .expect("the session must leave agent_session_opened in the log");
    assert_eq!(opened.0.as_ref().map(|n| n.as_str()), Some("work"));
    assert!(!opened.1.session_id.as_str().is_empty());
    assert_eq!(
        opened.1.model.as_ref().map(|model| model.as_str()),
        Some("mock-model")
    );
}

#[tokio::test]
async fn agent_messages_are_bounded_summaries_that_never_carry_note_content() {
    let bench = Bench::new();
    let fixture = r#"
sessions:
  - steps:
      - { type: note, text: "thinking about SECRET-TOKEN-123 carefully" }
      - { type: usage, input_tokens: 40, output_tokens: 10 }
      - { type: tool_use, name: edit, target_digest: abc123 }
    outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _) = bench.run(SESSION_EVENTS_WORKFLOW, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let messages: Vec<&yunta_core::events::AgentMessagePayload> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::AgentMessage(p)) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 3, "one agent_message per adapter event");

    // Never the content — a mechanical size+digest summary only.
    let jsonl = serde_json::to_string(&messages).unwrap();
    assert!(
        !jsonl.contains("SECRET-TOKEN-123"),
        "note content must never be persisted: {jsonl}"
    );
    let note = messages
        .iter()
        .find(|m| m.message_type == yunta_core::events::AgentMessageType::Note)
        .unwrap();
    let summary = note.text.as_deref().unwrap_or_default();
    assert_eq!(summary, "41 bytes, sha256 052f9656f4d7");

    let usage = messages
        .iter()
        .find(|m| m.message_type == yunta_core::events::AgentMessageType::Usage)
        .unwrap();
    assert_eq!(usage.input_tokens, Some(40));
    assert_eq!(usage.output_tokens, Some(10));

    let tool = messages
        .iter()
        .find(|m| m.message_type == yunta_core::events::AgentMessageType::ToolUse)
        .unwrap();
    assert_eq!(tool.tool_name.as_deref(), Some("edit"));
    assert_eq!(tool.target_digest.as_deref(), Some("abc123"));
}

#[tokio::test]
async fn resolved_skills_reach_the_session_request_always_first() {
    let bench = Bench::new();
    install_skill(&bench.worktree, "conventions");
    install_skill(&bench.worktree, "grill");
    let fixture = r#"
capabilities: { skills: true }
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _, adapter) =
        run_with_recording_mock(&bench, SKILLS_WORKFLOW, fixture, SKILLS_CONFIG).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let seen = adapter.skills_seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0],
        vec![
            bench.worktree.join(".yunta/skills/conventions"),
            bench.worktree.join(".yunta/skills/grill"),
        ],
        "skills.always mounts first, then the node's own list"
    );
}

#[tokio::test]
async fn a_missing_skill_name_fails_the_node_with_where_it_looked() {
    let bench = Bench::new();
    install_skill(&bench.worktree, "conventions");
    // `grill` is never installed.
    let fixture = r#"
capabilities: { skills: true }
sessions:
  - outcome: { type: completed, summary: "never reached" }
"#;
    let (terminal, state, adapter) =
        run_with_recording_mock(&bench, SKILLS_WORKFLOW, fixture, SKILLS_CONFIG).await;
    match &terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(
                *reason,
                format!(
                    "node `work` failed: skill `grill` not found under `{}` — add the directory, fix `skills.paths` in the config, or install the pack that declares it",
                    bench.worktree.join(".yunta/skills").display()
                )
            );
        }
        other => panic!("a missing skill must fail the node, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.get("work"),
        Some(NodeState::Failed { .. })
    ));
    assert!(adapter.skills_seen().is_empty(), "no session was spawned");
}

#[tokio::test]
async fn an_adapter_without_the_skills_capability_degrades_with_an_event() {
    let bench = Bench::new();
    install_skill(&bench.worktree, "conventions");
    install_skill(&bench.worktree, "grill");
    // Default capabilities: `skills: false`.
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _, adapter) =
        run_with_recording_mock(&bench, SKILLS_WORKFLOW, fixture, SKILLS_CONFIG).await;
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "a skill is not correctness"
    );

    assert_eq!(
        adapter.skills_seen(),
        vec![Vec::<std::path::PathBuf>::new()],
        "the engine never populates skills an adapter didn't declare"
    );
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let degraded = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p)) => Some(p),
            _ => None,
        })
        .expect("the degradation must be an event, never silence");
    assert_eq!(degraded.capability, "skills");
    assert_eq!(degraded.adapter, "mock");
}

#[tokio::test]
async fn distill_copies_declared_artifacts_with_provenance_and_commits() {
    let bench = Bench::new();
    let fixture = distill_fixture(&bench.run_dir().join("artifacts"));
    let (terminal, _) = bench.run(DISTILL_WORKFLOW, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let dest = bench
        .worktree
        .join(".yunta/knowledge/distilled/distiller")
        .join(bench.run_id.as_str());
    let copied = std::fs::read_to_string(dest.join("plan.md")).expect("the artifact must land");
    assert_eq!(copied, "DISTILLED-MARKER: the durable decision\n");

    let provenance: serde_norway::Value =
        serde_norway::from_str(&std::fs::read_to_string(dest.join("provenance.yaml")).unwrap())
            .unwrap();
    assert_eq!(
        provenance["source_run"].as_str(),
        Some(bench.run_id.as_str())
    );
    assert_eq!(provenance["workflow"].as_str(), Some("distiller"));
    assert!(provenance["artifacts"][0]["content_hash"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(
        provenance["verification"]["findings"]["blocking"].as_u64(),
        Some(0)
    );

    // The knowledge travels on the run's own branch: a conventional
    // commit exists in the worktree.
    let log = std::process::Command::new("git")
        .args(["log", "--oneline", "-3"])
        .current_dir(&bench.worktree)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    assert!(
        log.contains(&format!("docs(knowledge): distill from {}", bench.run_id)),
        "got: {log}"
    );
}

#[tokio::test]
async fn a_distill_path_never_produced_becomes_a_finding_and_the_rest_lands() {
    let bench = Bench::new();
    let workflow = r#"
name: distiller
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the plan to {{run.dir}}/artifacts/plan.md."
    artifacts:
      produces: [plan.md]
  - id: notes
    kind: prompt
    runner: executor
    depends_on: [plan]
    prompt: "Maybe write notes."
    artifacts:
      produces: [notes.md]
on_finish:
  - distill: [plan.md, notes.md]
"#;
    // `notes` fails before producing its artifact — but with a re-route
    // budget of zero the run pauses... instead: notes produces, then we
    // delete it? Simpler: notes' fixture writes the artifact and the run
    // finishes, then this test only covers the produced path. The
    // missing-path case uses a workflow whose declared artifact the
    // session legitimately produced but distill names one more — which
    // check would refuse. So: simulate runtime-missing by removing the
    // file after the run? No — distill runs inside execute_run. The
    // honest runtime-missing case: `notes` is mode-excluded.
    let workflow = workflow.replace(
        "on_finish:",
        "modes:\n  quick: { include: [plan] }\n  full: { include: all }\non_finish:",
    );
    let artifacts = bench.run_dir().join("artifacts");
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.md", content: "plan\n" }}
    outcome: {{ type: completed, summary: "planned" }}
"#,
        artifacts = artifacts.display()
    );
    // Run in `quick` mode: `notes` never runs, its artifact never
    // exists, but distill declares it.
    let workflow_parsed: Workflow = serde_norway::from_str(&workflow).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow_parsed,
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
            mode: &"quick".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    let adapter = MockAdapter::from_yaml(&fixture).unwrap();
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), Arc::new(adapter));
    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
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
    assert_eq!(report.terminal, RunTerminal::Finished);

    let dest = bench
        .worktree
        .join(".yunta/knowledge/distilled/distiller")
        .join(bench.run_id.as_str());
    assert!(dest.join("plan.md").exists(), "the produced path lands");
    assert!(!dest.join("notes.md").exists());

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let finding = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::FindingPosted(p)) => Some(&p.finding),
            _ => None,
        })
        .expect("the missing path must become a finding, never be lost");
    assert!(finding.title.contains("notes.md"), "got: {finding:?}");
    assert_eq!(finding.severity, yunta_core::events::FindingSeverity::Minor);

    let provenance: serde_norway::Value =
        serde_norway::from_str(&std::fs::read_to_string(dest.join("provenance.yaml")).unwrap())
            .unwrap();
    let listed: Vec<&str> = provenance["artifacts"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(listed, vec!["plan.md", "notes.md"]);
    assert_eq!(provenance["artifacts"][1]["missing"].as_bool(), Some(true));
}

#[tokio::test]
async fn a_paused_run_distills_nothing() {
    let bench = Bench::new();
    let workflow = r#"
name: distiller
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the plan to {{run.dir}}/artifacts/plan.md."
    artifacts:
      produces: [plan.md]
  - id: boom
    kind: bash
    depends_on: [plan]
    run: "false"
on_finish:
  - distill: [plan.md]
"#;
    let fixture = distill_fixture(&bench.run_dir().join("artifacts"));
    let (terminal, _) = bench.run(workflow, &fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert!(
        !bench.worktree.join(".yunta/knowledge").exists(),
        "a paused run did not close — nothing distills"
    );
}

#[tokio::test]
async fn a_later_run_mounts_the_distilled_knowledge() {
    let bench = Bench::new();
    let fixture = distill_fixture(&bench.run_dir().join("artifacts"));
    let (terminal, _) = bench.run(DISTILL_WORKFLOW, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    // Second run, same checkout: a knowledge context source must see
    // the distilled file — the loop closed, from budget limit to knowledge layering.
    let second_workflow = r#"
name: consumer
nodes:
  - id: ask
    kind: prompt
    runner: executor
    context:
      - knowledge: {}
    prompt: "Use what the team learned."
"#;
    let second_fixture = r#"
sessions:
  - match_prompt_contains: "DISTILLED-MARKER"
    outcome: { type: completed, summary: "informed" }
"#;
    let workflow: Workflow = serde_norway::from_str(second_workflow).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let second_id = RunId::from("run-test-2");
    let run_dir = create_run(
        CreateRunParams {
            run_id: &second_id,
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
    let adapter = MockAdapter::from_yaml(second_fixture).unwrap();
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), Arc::new(adapter));
    let report = execute_run(RunEnv {
        run_id: &second_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
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
        report.terminal,
        RunTerminal::Finished,
        "the consumer session only matches if the distilled content reached its prompt"
    );
}

// --- runners fan-out end-to-end + node-level agent ---------------------

#[tokio::test]
async fn a_fanout_review_runs_one_session_per_role_with_rendered_artifacts() {
    let bench = Bench::new();
    let workflow = r#"
name: fanout
nodes:
  - id: review
    kind: prompt
    runners: [reviewer, reviewer-alt]
    prompt: "Audit as {{runner.role}}; write {{run.dir}}/artifacts/findings-{{runner.role}}.md"
    artifacts:
      produces: ["findings-{{runner.role}}.md"]
"#;
    let config = r#"
runners:
  reviewer:
    - { adapter: mock, model: mock-model }
  reviewer-alt:
    - { adapter: mock, model: mock-model }
"#;
    let artifacts = bench.run_dir().join("artifacts");
    let fixture = format!(
        r#"
sessions:
  - match_prompt_contains: "Audit as reviewer;"
    effects:
      - {{ path: "{artifacts}/findings-reviewer.md", content: "r1\n" }}
    outcome: {{ type: completed, summary: "reviewed" }}
  - match_prompt_contains: "Audit as reviewer-alt"
    effects:
      - {{ path: "{artifacts}/findings-reviewer-alt.md", content: "r2\n" }}
    outcome: {{ type: completed, summary: "reviewed-alt" }}
"#,
        artifacts = artifacts.display()
    );

    let (terminal, state) = bench.run_with_config(workflow, &fixture, config).await;
    assert_eq!(terminal, RunTerminal::Finished);
    for node in ["review@reviewer", "review@reviewer-alt"] {
        assert!(
            matches!(state.nodes.get(node), Some(NodeState::Finished { .. })),
            "node `{node}` should be finished, got {:?}",
            state.nodes.get(node)
        );
    }
    // The templated artifact names rendered per expanded node.
    assert!(artifacts.join("findings-reviewer.md").exists());
    assert!(artifacts.join("findings-reviewer-alt.md").exists());
}

#[tokio::test]
async fn a_node_level_agent_overrides_the_runner_candidate_s_agent() {
    let bench = Bench::new();
    let workflow = r#"
name: agent-override
nodes:
  - id: audit
    kind: prompt
    runner: reviewer
    agent: security-auditor
    prompt: "Audit."
"#;
    let config = r#"
runners:
  reviewer:
    - { adapter: mock, model: mock-model, agent: benito }
"#;
    let fixture = r#"
capabilities: { custom_agents: true }
sessions:
  - outcome: { type: completed, summary: "audited" }
"#;
    let (terminal, _, adapter) = run_with_recording_mock(&bench, workflow, fixture, config).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        adapter.agents_seen(),
        vec![Some("security-auditor".into())],
        "the node's own agent wins over the candidate's"
    );
}

#[tokio::test]
async fn max_per_run_holds_exactly_under_a_fully_concurrent_batch() {
    // Four tasks in ONE batch (`concurrency: 4`) all request an
    // expansion under `rules` with `max_per_run: 2`. The cap window
    // (read count → decide → commit) is atomic across the batch, so the
    // count is deterministic — exactly 2 granted, 2 escalated — never
    // "up to concurrency - 1 over".
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: capped-concurrency
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    concurrency: 4
    prompt: "Read your task from the ledger and implement it."
    scope_expansion:
      mode: rules
      within: ["extra-*.txt"]
      max_per_run: 2
"#;
    let mut ledger = String::from("tasks:\n");
    for n in 1..=4 {
        ledger.push_str(&task_yaml(
            &format!("task-{n}"),
            &format!("t{n}"),
            &format!("a{n}.txt"),
            &format!("test -f a{n}.txt"),
        ));
    }

    let mut fixture = plan_session(&artifacts_dir, &ledger);
    for n in 1..=4 {
        let request_yaml = format!(
            "paths:\n  - extra-{n}.txt\nreason: \"needs the extra file\"\nproposed_criterion:\n  cmd: \"test -f extra-{n}.txt\"\n"
        );
        fixture.push_str(&format!(
            "  - match_prompt_contains: \"task-{n}\"\n    effects:\n      - {{ path: a{n}.txt, content: \"a\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: did-{n} }}\n",
            yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
            request_yaml,
        ));
    }

    let (_terminal, _state) = bench.run(workflow, &fixture).await;

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let requested = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::ScopeExpansionRequested(_))
            )
        })
        .count();
    let granted = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::ScopeExpansionGranted(_))
            )
        })
        .count();
    assert_eq!(
        requested, 4,
        "every batch member's request must be recorded"
    );
    assert_eq!(
        granted, 2,
        "max_per_run: 2 must hold exactly under a concurrent batch"
    );
}

// --- events.jsonl on the Broken path ----------------------------------

#[tokio::test]
async fn a_broken_log_still_exports_events_jsonl_for_forensics() {
    let bench = Bench::new();
    let workflow = r#"
name: broken
nodes:
  - id: build
    kind: bash
    run: "true"
"#;
    // Corrupt the log by hand: a node_finished with no node_started —
    // exactly the class of inconsistency `derive` refuses to guess over.
    let wf: Workflow = serde_norway::from_str(workflow).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &wf,
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
    bench
        .storage
        .append(
            &yunta_core::events::EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("ghost".into()),
                payload: yunta_core::events::EventPayload::NodeFinished(
                    yunta_core::events::NodeFinishedPayload {
                        outcome: "??".to_string(),
                        tokens_used: yunta_core::events::TokenUsage::default(),
                    },
                ),
            },
            &yunta_core::SystemClock,
        )
        .unwrap();

    let adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    let result = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
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
    .await;

    assert!(
        matches!(result, Err(yunta_engine::RunError::Broken { .. })),
        "a corrupt log is a Broken error, got {result:?}"
    );
    // The corrupt log is exactly the one you most want exported — the
    // forensic copy exists even though the run errored.
    let exported = std::fs::read_to_string(run_dir.join("events.jsonl")).unwrap();
    let kinds: Vec<String> = exported
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .unwrap_or_else(|e| panic!("each exported line is canonical JSON ({line:?}): {e}"))
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .expect("each event line names its kind")
                .to_string()
        })
        .collect();
    assert_eq!(
        kinds,
        ["run_created", "node_finished", "run_resumed"],
        "the forensic export preserves every event even though the run errored: {exported}"
    );
}

#[tokio::test]
async fn an_orphaned_prompt_with_resume_session_continues_the_same_session() {
    let fixture = r#"
capabilities: { resume_session: true }
sessions:
  - outcome: { type: completed, summary: "picked up where it left off" }
"#;
    let (terminal, events, adapter) =
        resume_orphan_with_mock(RESUME_WORKFLOW, fixture, Some("mock-session-orig")).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        adapter.resumes_seen(),
        vec![yunta_core::SessionId::from("mock-session-orig")],
        "the cut session must be resumed, not replaced"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p)) if p.capability == "resume_session"
        )),
        "a successful resume degrades nothing"
    );
}

#[tokio::test]
async fn resume_session_without_the_capability_degrades_to_restart_with_an_event() {
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "fresh session" }
"#;
    let (terminal, events, adapter) =
        resume_orphan_with_mock(RESUME_WORKFLOW, fixture, Some("mock-session-orig")).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(adapter.resumes_seen().is_empty());
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p)) if p.capability == "resume_session"
                    && p.policy_applied.contains("restart_node")
        )),
        "degrading to a fresh session must be an event, never a silence"
    );
}

#[tokio::test]
async fn resume_session_with_no_recorded_session_restarts_with_an_event() {
    let fixture = r#"
capabilities: { resume_session: true }
sessions:
  - outcome: { type: completed, summary: "fresh session" }
"#;
    let (terminal, events, adapter) = resume_orphan_with_mock(RESUME_WORKFLOW, fixture, None).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(adapter.resumes_seen().is_empty());
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p)) if p.capability == "resume_session"
                    && p.policy_applied.contains("no session")
        )),
        "a crash before the session opened restarts WITH an explicit event"
    );
}

#[tokio::test]
async fn a_clean_first_run_under_resume_session_spawns_normally_without_events() {
    let fixture = r#"
capabilities: { resume_session: true }
sessions:
  - outcome: { type: completed, summary: "first run" }
"#;
    let bench = Bench::new();
    let (terminal, _state, adapter) =
        run_with_recording_mock(&bench, RESUME_WORKFLOW, fixture, MOCK_CONFIG).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(adapter.resumes_seen().is_empty());
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::CapabilityDegraded(_))
    )));
}

/// A `prompt` node that declares `network: false` — the policy no adapter
/// can enforce.
const NETWORK_DECLARED_WORKFLOW: &str = r#"
name: network-declared
nodes:
  - id: work
    kind: prompt
    runner: executor
    network: false
    prompt: "Do the thing."
"#;

#[tokio::test]
async fn network_false_is_reported_as_declarative_only() {
    // `network: false` is a declared policy, not a capability the engine has:
    // no adapter isolates the network (D105/D119). The engine records the gap
    // before the session with a `capability_degraded`, never consolidating it
    // into silence.
    let bench = Bench::new();
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _) = bench.run(NETWORK_DECLARED_WORKFLOW, fixture).await;
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "network: false blocks nothing — policy, not capability"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let degraded = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p))
                if p.capability == "network_isolation" =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("network: false the adapter cannot enforce must be an event, never silence");
    assert_eq!(degraded.adapter, "mock");
    assert_eq!(
        degraded.policy_applied,
        "declarative-only — the adapter declares no network isolation; `network: false` is recorded for policy and audit, not enforced"
    );
}

#[tokio::test]
async fn a_node_that_declares_no_network_policy_records_no_isolation_degradation() {
    // Only an explicit `network: false` is a policy the engine reports it
    // cannot enforce; a node that never mentions network declares none, so
    // there is nothing to degrade.
    let bench = Bench::new();
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _) = bench.run(SESSION_EVENTS_WORKFLOW, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p))
                if p.capability == "network_isolation"
        )),
        "an unset network policy is not a degradation"
    );
}

#[tokio::test]
async fn an_adapter_that_isolates_the_network_records_no_degradation() {
    // The degradation is capability-aware: an adapter that declares
    // `network_isolation` can enforce `network: false`, so the engine records
    // nothing — the policy is applied, not degraded.
    let bench = Bench::new();
    let fixture = r#"
capabilities: { network_isolation: true }
sessions:
  - outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _, _adapter) =
        run_with_recording_mock(&bench, NETWORK_DECLARED_WORKFLOW, fixture, MOCK_CONFIG).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::CapabilityDegraded(p))
                if p.capability == "network_isolation"
        )),
        "an adapter that declares network isolation leaves nothing to degrade"
    );
}
