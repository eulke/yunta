//! Sessions end-to-end: session events, the skills chain, on_finish distillation, runner fan-out, forensic events.jsonl, and resume_session.

use std::collections::HashMap;
use std::sync::Arc;

use yunta_adapters::MockAdapter;
use yunta_core::port::Adapter;
use yunta_core::{AdapterId, ConfigLayer, RunId, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_testkit::{Bench, MOCK_CONFIG};
use yunta_testkit_core::FixedClock;

mod common;
use common::*;
use yunta_core::events::{ArtifactEvent, FindingEvent, NodeEvent, ScopeEvent, SessionEvent};

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
            Some(yunta_core::events::EventPayload::Session(SessionEvent::Opened(p))) => {
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
      - { type: tool_use, name: edit, target: abc123 }
    outcome: { type: completed, summary: "done" }
"#;
    let (terminal, _) = bench.run(SESSION_EVENTS_WORKFLOW, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let messages: Vec<&yunta_core::events::AgentMessagePayload> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Session(SessionEvent::Message(p))) => Some(p),
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
    assert_eq!(
        tool.target.as_ref().map(|target| target.digest.clone()),
        Some(yunta_core::sha256_hex(b"abc123")),
        "the mock hands over what the fixture scripted, identified by its hash"
    );
    assert_eq!(
        tool.target
            .as_ref()
            .and_then(|target| target.display.clone()),
        None,
        "and never shown: a tool's argument is the session's own text"
    );
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
        state.nodes.state("work"),
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
            Some(yunta_core::events::EventPayload::Session(SessionEvent::CapabilityDegraded(
                p,
            ))) => Some(p),
            _ => None,
        })
        .expect("the degradation must be an event, never silence");
    assert_eq!(degraded.capability, yunta_core::Capability::Skills);
    assert_eq!(degraded.adapter, "mock");
}

#[tokio::test]
async fn distill_copies_declared_artifacts_with_provenance_and_commits() {
    let bench = Bench::new();
    let fixture = distill_fixture(&bench);
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
    prompt: "Write the plan to {{node.artifacts}}/plan.md."
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
  - distill: [{ node: plan, name: plan.md }, { node: notes, name: notes.md }]
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
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{artifacts}/plan.md", content: "plan\n" }}
    outcome: {{ type: completed, summary: "planned" }}
"#,
        artifacts = bench.staging("plan").display()
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
    .await
    .unwrap()
    .manifest;
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"quick".into(),
            worktree: &bench.worktree,
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
        secrets: None,
        observer: None,
        fence_hook: None,
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
            Some(yunta_core::events::EventPayload::Findings(FindingEvent::Posted(p))) => {
                Some(&p.finding)
            }
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
    prompt: "Write the plan to {{node.artifacts}}/plan.md."
    artifacts:
      produces: [plan.md]
  - id: boom
    kind: bash
    depends_on: [plan]
    run: "false"
on_finish:
  - distill: [{ node: plan, name: plan.md }]
"#;
    let fixture = distill_fixture(&bench);
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
    let fixture = distill_fixture(&bench);
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
    .await
    .unwrap()
    .manifest;
    let second_id = RunId::from("run-test-2");
    let run_dir = create_run(
        CreateRunParams {
            run_id: &second_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            worktree: &bench.worktree,
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
        secrets: None,
        observer: None,
        fence_hook: None,
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
    prompt: "Audit as {{runner.role}}; write {{node.artifacts}}/findings-{{runner.role}}.md"
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
    // Each fan-out sibling is a node of its own, so each writes in a
    // directory of its own.
    let fixture = format!(
        r#"
sessions:
  - match_prompt_contains: "Audit as reviewer;"
    effects:
      - {{ path: "{reviewer}/findings-reviewer.md", content: "r1\n" }}
    outcome: {{ type: completed, summary: "reviewed" }}
  - match_prompt_contains: "Audit as reviewer-alt"
    effects:
      - {{ path: "{alt}/findings-reviewer-alt.md", content: "r2\n" }}
    outcome: {{ type: completed, summary: "reviewed-alt" }}
"#,
        reviewer = bench.staging("review@reviewer").display(),
        alt = bench.staging("review@reviewer-alt").display()
    );

    let (terminal, state) = bench.run_with_config(workflow, &fixture, config).await;
    assert_eq!(terminal, RunTerminal::Finished);
    for node in ["review@reviewer", "review@reviewer-alt"] {
        assert!(
            matches!(state.nodes.state(node), Some(NodeState::Finished { .. })),
            "node `{node}` should be finished, got {:?}",
            state.nodes.state(node)
        );
    }
    // The templated artifact names rendered per expanded node, each
    // held by the sibling that produced it.
    for role in ["reviewer", "reviewer-alt"] {
        assert!(
            bench
                .projection(
                    Some(&format!("review@{role}")),
                    &format!("findings-{role}.md")
                )
                .is_ok(),
            "`review@{role}` holds its own rendered artifact"
        );
    }
}

/// Two fan-out siblings each declare `findings` — the same kind, with no
/// template between them — and each holds its own.
///
/// The identity is `(node, kind)` and a fan-out sibling is a node of its
/// own, so nothing has to be spelled per role for the two documents to
/// stay apart: the template the reference workflows carried existed only
/// because `artifacts/` was flat.
#[tokio::test]
async fn two_fanout_siblings_each_hold_their_own_document_of_one_kind() {
    let bench = Bench::new();
    let workflow = r#"
name: fanout-findings
nodes:
  - id: review
    kind: prompt
    runners: [reviewer, reviewer-alt]
    prompt: "Audit as {{runner.role}} and report what you find."
    artifacts:
      produces: [findings]
"#;
    let config = r#"
runners:
  reviewer:
    - { adapter: mock, model: mock-model }
  reviewer-alt:
    - { adapter: mock, model: mock-model }
"#;
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - match_prompt_contains: "Audit as reviewer and"
    steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments: { id: f-one, severity: minor, title: One, location: src/a.rs, detail: "the first" }
    outcome: { type: completed, summary: "reviewed" }
  - match_prompt_contains: "Audit as reviewer-alt and"
    steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments: { id: f-two, severity: minor, title: Two, location: src/b.rs, detail: "the second" }
    outcome: { type: completed, summary: "reviewed-alt" }
"#;

    let (terminal, state) = bench.run_with_config(workflow, fixture, config).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    // One acceptance per sibling, under the same identity and different
    // producers — and each view sits under its own node, named by the
    // kind.
    let held = bench.accepted();
    let findings: Vec<_> = held
        .iter()
        .filter(|a| {
            a.artifact
                == yunta_core::events::ArtifactId::Interpreted {
                    kind: yunta_core::ArtifactKind::Findings,
                }
        })
        .collect();
    assert_eq!(findings.len(), 2, "one per sibling: {held:?}");
    for (role, id) in [("reviewer", "f-one"), ("reviewer-alt", "f-two")] {
        let node = format!("review@{role}");
        assert!(
            findings
                .iter()
                .any(|a| a.producer.as_ref().map(|n| n.as_str()) == Some(node.as_str())),
            "`{node}` holds its own findings artifact: {findings:?}"
        );
        let bytes = bench
            .projection(Some(&node), "findings.yaml")
            .unwrap_or_else(|e| panic!("`{node}`'s view is named by its kind: {e}"));
        let file: yunta_core::FindingsFile =
            yunta_core::shape::read(&bytes, "findings.yaml").expect("a canonical findings file");
        assert_eq!(
            file.findings
                .iter()
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>(),
            vec![id],
            "each sibling's document is its own"
        );
    }
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

    let workflow = r#"
name: capped-concurrency
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    concurrency: 4
    prompt: "Read your task from the tasks document and implement it."
    scope_expansion:
      mode: rules
      within: ["extra-*.txt"]
      max_per_run: 2
"#;
    let mut tasks = String::from("tasks:\n");
    for n in 1..=4 {
        tasks.push_str(&task_yaml(
            &format!("task-{n}"),
            &format!("t{n}"),
            &format!("a{n}.txt"),
            &format!("test -f a{n}.txt"),
        ));
    }

    let mut fixture = plan_session(&tasks);
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
                Some(yunta_core::events::EventPayload::Scope(
                    ScopeEvent::Requested(_)
                ))
            )
        })
        .count();
    let granted = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::Scope(
                    ScopeEvent::Granted(_)
                ))
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
    .await
    .unwrap()
    .manifest;
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            worktree: &bench.worktree,
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
                payload: yunta_core::events::EventPayload::Node(NodeEvent::Finished(
                    yunta_core::events::NodeFinishedPayload::new(
                        "??".to_string(),
                        yunta_core::events::TokenUsage::default(),
                    ),
                )),
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
        secrets: None,
        observer: None,
        fence_hook: None,
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
    let (terminal, events, adapter) = resume_orphan_with_mock(Orphan {
        workflow: RESUME_WORKFLOW,
        fixture,
        session: Some("mock-session-orig"),
        staged: &[],
    })
    .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        adapter.resumes_seen(),
        vec![yunta_core::SessionId::from("mock-session-orig")],
        "the cut session must be resumed, not replaced"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::Session(SessionEvent::CapabilityDegraded(p))) if p.capability == yunta_core::Capability::ResumeSession
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
    let (terminal, events, adapter) = resume_orphan_with_mock(Orphan {
        workflow: RESUME_WORKFLOW,
        fixture,
        session: Some("mock-session-orig"),
        staged: &[],
    })
    .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(adapter.resumes_seen().is_empty());
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::Session(SessionEvent::CapabilityDegraded(p))) if p.capability == yunta_core::Capability::ResumeSession
                    && p.policy_applied().contains("restart_node")
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
    let (terminal, events, adapter) = resume_orphan_with_mock(Orphan {
        workflow: RESUME_WORKFLOW,
        fixture,
        session: None,
        staged: &[],
    })
    .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(adapter.resumes_seen().is_empty());
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::Session(SessionEvent::CapabilityDegraded(p))) if p.capability == yunta_core::Capability::ResumeSession
                    && p.policy_applied() == yunta_core::events::Policy::FreshSession.to_string()
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
        Some(yunta_core::events::EventPayload::Session(
            SessionEvent::CapabilityDegraded(_)
        ))
    )));
}

/// A resumable `prompt` node that owes a file of its own: what the
/// session writes is what the close reads back.
const RESUME_ARTIFACT_WORKFLOW: &str = r#"
name: resumable-artifact
nodes:
  - id: work
    kind: prompt
    runner: executor
    on_interrupt: resume_session
    prompt: "Write the report."
    artifacts: { produces: [report.md] }
"#;

/// What the cut session had already written before the interruption.
const REPORT_LEFT_BY_THE_CUT_SESSION: &[(&str, &str, &str)] =
    &[("work", "report.md", "the report the cut session wrote\n")];

#[tokio::test]
async fn a_continued_session_keeps_the_artifact_it_had_already_written() {
    // The staging belongs to the session, not to the attempt: the
    // session picked back up is the one that wrote `report.md`, so that
    // file is work it did. It does not write it again, and the node
    // closes on it.
    let fixture = r#"
capabilities: { resume_session: true }
sessions:
  - outcome: { type: completed, summary: "finished what it had started" }
"#;
    let (terminal, events, adapter) = resume_orphan_with_mock(Orphan {
        workflow: RESUME_ARTIFACT_WORKFLOW,
        fixture,
        session: Some("mock-session-orig"),
        staged: REPORT_LEFT_BY_THE_CUT_SESSION,
    })
    .await;

    assert_eq!(
        adapter.resumes_seen(),
        vec![yunta_core::SessionId::from("mock-session-orig")],
        "the cut session must be resumed, not replaced"
    );
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "what the continued session already wrote closes the node"
    );
    let accepted: Vec<String> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Artifacts(ArtifactEvent::Accepted(p))) => {
                Some(p.artifact.to_string())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        accepted,
        vec!["report.md".to_string()],
        "the run holds the artifact the continued session wrote"
    );
}

#[tokio::test]
async fn a_fresh_session_replacing_an_interrupted_one_opens_on_an_empty_staging() {
    // The adapter declares no session resume, so the interrupted session
    // is replaced rather than continued. This session wrote nothing:
    // what the one before it left is not its work, and the node owes an
    // artifact it never produced.
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "started over" }
"#;
    let (terminal, _events, adapter) = resume_orphan_with_mock(Orphan {
        workflow: RESUME_ARTIFACT_WORKFLOW,
        fixture,
        session: Some("mock-session-orig"),
        staged: REPORT_LEFT_BY_THE_CUT_SESSION,
    })
    .await;

    assert!(adapter.resumes_seen().is_empty());
    let RunTerminal::Paused { reason } = &terminal else {
        panic!("a fresh session is judged on what it produced: {terminal:?}");
    };
    assert!(
        reason.contains("report.md") && reason.contains("never produced"),
        "nothing the replaced session left becomes this session's artifact: {reason}"
    );
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
            Some(yunta_core::events::EventPayload::Session(SessionEvent::CapabilityDegraded(
                p,
            ))) if p.capability == yunta_core::Capability::NetworkIsolation => Some(p),
            _ => None,
        })
        .expect("network: false the adapter cannot enforce must be an event, never silence");
    assert_eq!(degraded.adapter, "mock");
    assert_eq!(
        degraded.policy_applied(),
        yunta_core::events::Policy::NetworkOpen.to_string()
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
            Some(yunta_core::events::EventPayload::Session(SessionEvent::CapabilityDegraded(p)))
                if p.capability == yunta_core::Capability::NetworkIsolation
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
            Some(yunta_core::events::EventPayload::Session(SessionEvent::CapabilityDegraded(p)))
                if p.capability == yunta_core::Capability::NetworkIsolation
        )),
        "an adapter that declares network isolation leaves nothing to degrade"
    );
}

#[tokio::test]
async fn distill_carries_the_hash_the_log_names_never_a_rehash_of_the_view() {
    // The view is a projection, and a node that overwrites one changes
    // no fact of the run: what distill copies and what it records as the
    // content hash both come from the artifact the log holds.
    let bench = Bench::new();
    // The view belongs to the engine, so the node that overwrites one
    // names it by its absolute path rather than through a template no
    // workflow has for it.
    let workflow = format!(
        r#"
name: distiller
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the plan to {{{{node.artifacts}}}}/plan.md."
    artifacts:
      produces: [plan.md]
  - id: tamper
    kind: bash
    depends_on: [plan]
    run: "echo TAMPERED-VIEW > {view}/plan/plan.md"
on_finish:
  - distill: [{{ node: plan, name: plan.md }}]
"#,
        view = bench.run_dir().join(yunta_core::ARTIFACTS_DIR).display()
    );
    let fixture = distill_fixture(&bench);
    let (terminal, state) = bench.run(&workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    let dest = bench
        .worktree
        .join(".yunta/knowledge/distilled/distiller")
        .join(bench.run_id.as_str());
    assert_eq!(
        std::fs::read_to_string(dest.join("plan.md")).expect("the artifact must land"),
        "DISTILLED-MARKER: the durable decision\n",
        "distill copies the bytes the run accepted, not what is lying in the view"
    );

    let held = bench.accepted();
    assert_eq!(held.len(), 1, "{held:?}");
    let provenance: serde_norway::Value =
        serde_norway::from_str(&std::fs::read_to_string(dest.join("provenance.yaml")).unwrap())
            .unwrap();
    assert_eq!(
        provenance["artifacts"][0]["content_hash"].as_str(),
        Some(format!("sha256:{}", held[0].content_hash).as_str()),
        "the recorded hash is the one on the log, never a rehash"
    );
}

// --- where a session writes --------------------------------------------

#[tokio::test]
async fn a_node_that_declares_an_opaque_artifact_writes_in_its_own_staging_directory() {
    let bench = Bench::new();
    let workflow = r#"
name: staged
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the plan to {{node.artifacts}}/plan.md."
    artifacts:
      produces: [plan.md]
"#;
    let staging = bench.staging("plan");
    let fixture = format!(
        r#"
sessions:
  - match_prompt_contains: "{staging}/plan.md"
    effects:
      - {{ path: "{staging}/plan.md", content: "the plan\n" }}
    outcome: {{ type: completed, summary: "planned" }}
"#,
        staging = staging.display()
    );

    let (terminal, state) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert_eq!(
        bench.mock().artifact_dirs_seen(),
        vec![Some(staging)],
        "the writable root a session is granted is this node's own, never the run's view"
    );
    assert_eq!(
        bench.artifact("plan.md").expect("the run holds it"),
        b"the plan\n"
    );
}

/// A prompt node's session and a loop node's task session are opened by
/// the same door, so neither can quietly get a different contract.
///
/// They differ in exactly three things, all of them the session's own:
/// what it is asked to do, where it works, and where it may scaffold.
/// Everything else — the model, the agent, the permissions, the skills,
/// the secrets, the budget, the adapter settings, the tools endpoint's
/// presence, the artifact directory — comes from the node, and the two
/// nodes here declare the same runner.
#[tokio::test]
async fn a_prompt_session_and_a_task_session_are_opened_by_the_same_door() {
    let bench = Bench::new();
    let workflow = r#"
name: one-door
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#;
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-1", "t1", "a1.txt", "test -f a1.txt")
    );
    let mut fixture = plan_session(&tasks);
    fixture.push_str(
        "  - match_prompt_contains: \"task-1\"\n    effects:\n      - { path: a1.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-1 }\n",
    );

    let (terminal, _) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "the run reaches its end");

    let requests = bench.mock().requests_seen();
    assert_eq!(requests.len(), 2, "one prompt session, one task session");
    let (prompt, task) = (&requests[0], &requests[1]);

    assert_eq!(prompt.model, task.model, "the same runner, the same model");
    assert_eq!(prompt.agent, task.agent);
    assert_eq!(prompt.permissions, task.permissions);
    assert_eq!(prompt.skills, task.skills);
    assert_eq!(
        prompt.env.keys().collect::<Vec<_>>(),
        task.env.keys().collect::<Vec<_>>()
    );
    assert_eq!(prompt.budget, task.budget);
    assert_eq!(prompt.adapter_settings, task.adapter_settings);
    assert_eq!(
        prompt.run_tools_endpoint.is_some(),
        task.run_tools_endpoint.is_some(),
        "both sessions hold the run's tools, or neither does"
    );

    // And the three that are the session's own.
    assert_ne!(prompt.prompt, task.prompt);
    assert_ne!(prompt.cwd, task.cwd, "a task works in its own worktree");
    assert_ne!(
        prompt.scratch_dir, task.scratch_dir,
        "concurrent sessions never scaffold over each other"
    );
}

/// What a tasks document noted about a task reaches the session that
/// implements it. `notes` is context for a runner with no history of
/// this repo; a brief that left it out asked the session to work
/// without the one thing the document wrote down for it.
#[tokio::test]
async fn a_task_brief_carries_the_notes_its_document_wrote() {
    let bench = Bench::new();
    let workflow = r#"
name: noted
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;
    let tasks = "tasks:\n  - id: task-1\n    title: \"Write a1\"\n    notes: \"the parser lives in src/lex.rs\"\n    scope: [\"a1.txt\"]\n    criteria:\n      - cmd: \"test -f a1.txt\"\n";
    let mut fixture = plan_session(tasks);
    fixture.push_str(
        "  - match_prompt_contains: \"task-1\"\n    effects:\n      - { path: a1.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-1 }\n",
    );

    let (terminal, _) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let brief = bench
        .mock()
        .requests_seen()
        .into_iter()
        .map(|request| request.prompt)
        .find(|prompt| prompt.contains("task-1"))
        .expect("the task session's brief");
    assert!(
        brief.contains("the parser lives in src/lex.rs"),
        "the brief carries what the document noted: {brief}"
    );
}

/// Every session carries what it may write, always: a node that
/// declared a scope carries it, and a `read_only` node carries a
/// ceiling that admits nothing under the worktree.
#[tokio::test]
async fn every_session_carries_its_fence_and_read_only_allows_nothing_under_the_worktree() {
    let bench = Bench::new();
    let workflow = r#"
name: two-nodes
nodes:
  - id: scoped
    kind: prompt
    runner: executor
    prompt: "Do the scoped thing."
    scope: ["src/**"]
  - id: reading
    kind: prompt
    runner: executor
    depends_on: [scoped]
    permissions: read-only
    prompt: "Read the thing."
"#;
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "scoped" }
  - outcome: { type: completed, summary: "reading" }
"#;
    let (terminal, _) = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let requests = bench.mock().requests_seen();
    assert_eq!(
        requests[0].fence.allowed.as_deref(),
        Some(["src/**".into()].as_slice()),
        "the node's declared scope is the ceiling it works to"
    );
    assert_eq!(
        requests[1].fence.allowed.as_deref(),
        Some([].as_slice()),
        "a read-only node may write nothing under the worktree"
    );
}

/// A session that can ask for more scope is told to ask; one that
/// cannot is told to report the need and move on.
#[tokio::test]
async fn a_task_session_advises_expansion_and_a_prompt_session_advises_a_finding() {
    let bench = Bench::new();
    let workflow = r#"
name: plan-then-work
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write the tasks."
    artifacts:
      produces: [tasks]
  - id: work
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#;
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-1", "t1", "a1.txt", "test -f a1.txt")
    );
    let mut fixture = plan_session(&tasks);
    fixture.push_str(
        "  - match_prompt_contains: \"task-1\"\n    effects:\n      - { path: a1.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-1 }\n",
    );

    let (terminal, _) = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let requests = bench.mock().requests_seen();
    assert_eq!(
        requests[0].fence.advice,
        yunta_core::fence::Advice::ReportFinding,
        "a prompt session has no tool to ask with"
    );
    assert_eq!(
        requests[1].fence.advice,
        yunta_core::fence::Advice::RequestExpansion,
        "a task session mounted the tool that asks"
    );
}
