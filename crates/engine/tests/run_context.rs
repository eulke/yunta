//! Context assembly: sources, templates, stable-first hashing, knowledge layering across repo/user/org, and loop-level context.

use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{write, Bench, MOCK_CONFIG};

mod common;
use common::*;
use yunta_core::events::NodeEvent;

#[tokio::test]
async fn a_files_source_resolves_a_literal_path_and_is_replayable() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("a.txt"), "MARKER-FILES-CONTENT\n").unwrap();

    let workflow = context_workflow("      - files: [\"a.txt\"]\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-FILES-CONTENT\"\n    outcome: { type: completed, summary: ok }\n";

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].kind, "files");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn a_command_source_resolves_stdout_and_is_replayable() {
    let bench = Bench::new();
    let workflow = context_workflow("      - command: \"echo MARKER-COMMAND-OUTPUT\"\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-COMMAND-OUTPUT\"\n    outcome: { type: completed, summary: ok }\n";

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    assert_eq!(sources[0].kind, "command");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn an_artifact_source_creates_an_implicit_dependency_and_resolves_the_content() {
    let bench = Bench::new();

    // No explicit `depends_on` on `plan` — the ordering must come purely
    // from `context: [{ artifact: { node: grill } }]`.
    let workflow = r#"
name: ctx-artifact
nodes:
  - id: grill
    kind: prompt
    runner: executor
    prompt: "Write the brief."
    artifacts:
      produces: [brief.md]
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Plan from the brief."
    context:
      - artifact: { node: grill, name: brief.md }
"#;
    let fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/brief.md\", content: \"MARKER-ARTIFACT-CONTENT\" }}\n    outcome: {{ type: completed, summary: grilled }}\n  - match_prompt_contains: \"MARKER-ARTIFACT-CONTENT\"\n    outcome: {{ type: completed, summary: planned }}\n",
        bench.staging("grill").display()
    );

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(workflow, &fixture).await;
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "the implicit dependency must order grill before plan without any explicit depends_on"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "plan");
    assert_eq!(sources[0].kind, "artifact");
    assert_materialized(&bench.run_dir(), &sources[0]);

    // One store, not two: the bytes `context_assembled` names and the
    // bytes the artifact's own acceptance names are the same object,
    // held once under the hash both events carry.
    let held = bench.accepted();
    assert_eq!(held.len(), 1, "{held:?}");
    assert_eq!(held[0].content_hash, sources[0].content_hash);
    assert_eq!(
        bench.object(&sources[0].content_hash).expect("the object"),
        b"MARKER-ARTIFACT-CONTENT"
    );
}

#[tokio::test]
async fn a_missing_artifact_reference_fails_the_node_never_silently_empty() {
    // ✓ del Plan: "fuente caída = nodo failed" — referencing a real,
    // already-run node whose artifact was simply never produced.
    let bench = Bench::new();
    let workflow = r#"
name: ctx-missing-artifact
nodes:
  - id: grill
    kind: bash
    run: "true"
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Plan from the brief."
    depends_on: [grill]
    context:
      - artifact: { node: grill, name: brief.md }
"#;
    let fixture = "sessions: []";

    let RunReport { terminal, state } = bench.run(workflow, fixture).await;
    match &state.nodes.state("plan") {
        Some(yunta_engine::NodeState::Failed { failure, .. }) => {
            assert_eq!(failure.to_string(), "context `artifact:grill/brief.md` on node `plan`: the artifact `brief.md` (declared by node `grill`) was never produced — this run's log holds no such artifact");
        }
        other => panic!("expected plan to fail citing the missing artifact, got {other:?}"),
    }
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

/// A node reading one required file, and the one session that answers
/// only once the file's content is in its prompt.
fn reads_architecture() -> (String, &'static str) {
    (
        context_workflow("      - files: [\"docs/architecture.md\"]\n"),
        "sessions:\n  - match_prompt_contains: \"MARKER-ARCHITECTURE\"\n    outcome: { type: completed, summary: ok }\n",
    )
}

#[tokio::test]
async fn a_missing_file_fails_the_node_saying_where_the_next_attempt_looks() {
    // Where the file has to be for the next attempt — the run's own tree,
    // and the commit it starts from — not the checkout this attempt read.
    let bench = Bench::new();
    let (workflow, fixture) = reads_architecture();

    let RunReport { terminal, state } = bench.run(&workflow, fixture).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let Some(yunta_engine::NodeState::Failed { failure, .. }) = state.nodes.state("ask") else {
        panic!("expected `ask` to fail on the missing file");
    };
    assert_eq!(
        failure.to_string(),
        format!(
            "context `files:docs/architecture.md` on node `ask`: `docs/architecture.md` is not \
             in the run's tree, which starts from commit `{}` — a file that is not committed \
             there, or that git ignores, never reaches it; put it at `{}` and choose `retry`",
            bench.manifest().base_commit.abbreviated(),
            bench.worktree.join("docs/architecture.md").display()
        )
    );
}

#[tokio::test]
async fn a_missing_file_retried_after_it_is_put_in_place_finishes() {
    // Once a person puts the file where the failure said, `retry` runs the
    // node again instead of leaving the run parked on it.
    let bench = Bench::new();
    let (workflow, fixture) = reads_architecture();
    let RunReport { terminal, .. } = bench.run(&workflow, fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    write(
        &bench.worktree.join("docs/architecture.md"),
        "MARKER-ARCHITECTURE\n",
    );
    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, state } = bench.wake_on_fixture(fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("ask"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(context_sources(&events, "ask")[0].kind, "files");
}

#[tokio::test]
async fn a_run_events_source_resolves_filtered_failures_and_is_replayable() {
    let bench = Bench::new();
    let workflow = r#"
name: ctx-run-events
nodes:
  - id: lint
    kind: bash
    run: "false"
    on_failure: { goto: fix-lint, max_reroutes: 1 }
  - id: fix-lint
    kind: prompt
    runner: executor
    prompt: "Fix the lint errors."
    context:
      - run-events: { filter: failed }
"#;
    // The failures reach the prompt as canonical JSONL, so the event's
    // kind reads `node_failed` (not the Rust Debug `NodeFailed`).
    let fixture =
        "sessions:\n  - match_prompt_contains: \"node_failed\"\n    outcome: { type: completed, summary: tried }\n";

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(workflow, fixture).await;
    match terminal {
        RunTerminal::Paused { reason } => assert_eq!(
            reason,
            "node `lint` failed and its 1 re-route(s) to `fix-lint` are exhausted — exit 1"
        ),
        other => panic!(
            "lint stays red, so the run pauses once its single reroute is exhausted, got {other:?}"
        ),
    }

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "fix-lint");
    assert_eq!(sources[0].kind, "run-events");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn a_tasks_source_resolves_aggregate_task_state_and_is_replayable() {
    let bench = Bench::new();
    let workflow = r#"
name: ctx-tasks
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: audit
    kind: prompt
    runner: executor
    depends_on: [plan]
    prompt: "Summarize the tasks document."
    context:
      - tasks: {}
"#;
    let fixture = format!(
        "{}  - match_prompt_contains: \"task-x\"\n    outcome: {{ type: completed, summary: audited }}\n",
        plan_session(&format!(
            "tasks:\n{}",
            task_yaml("task-x", "x", "x.txt", "test -f x.txt")
        )),
    );

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "audit");
    assert_eq!(sources[0].kind, "tasks");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn a_knowledge_source_resolves_the_repo_layer_and_is_replayable() {
    let bench = Bench::new();
    std::fs::create_dir_all(bench.worktree.join(".yunta/knowledge")).unwrap();
    std::fs::write(
        bench.worktree.join(".yunta/knowledge/note.md"),
        "MARKER-KNOWLEDGE-CONTENT\n",
    )
    .unwrap();

    let workflow = context_workflow("      - knowledge: {}\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-KNOWLEDGE-CONTENT\"\n    outcome: { type: completed, summary: ok }\n";

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    assert_eq!(sources[0].kind, "knowledge");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn a_node_output_source_resolves_a_bash_node_s_captured_stdout() {
    let bench = Bench::new();
    let workflow = r#"
name: ctx-node-output
nodes:
  - id: build
    kind: bash
    run: "echo MARKER-BUILD-OUTPUT"
  - id: report
    kind: prompt
    runner: executor
    depends_on: [build]
    prompt: "Report on the build."
    context:
      - node-output: { node: build }
"#;
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-BUILD-OUTPUT\"\n    outcome: { type: completed, summary: reported }\n";

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "report");
    assert_eq!(sources[0].kind, "node-output");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

// --- templates — {{runner.name}}, {{node.id}}, {{project.*}} ---------

#[tokio::test]
async fn a_node_can_reference_its_own_runner_by_template() {
    // `{{runner.name}}` is the name the node declares under `runner:`,
    // known from the workflow alone — never the adapter or model a later
    // resolution picks.
    let bench = Bench::new();
    let workflow = r#"
name: runner-template
nodes:
  - id: only
    kind: bash
    runner: executor
    run: "test 'executor' = '{{runner.name}}'"
"#;
    let RunReport { terminal, state: _ } = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_node_can_reference_its_own_id_by_template() {
    let bench = Bench::new();
    let workflow = r#"
name: node-id-template
nodes:
  - id: only
    kind: bash
    run: "test 'only' = '{{node.id}}'"
"#;
    let RunReport { terminal, state: _ } = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_node_can_reference_project_config_by_template() {
    let bench = Bench::new();
    let config = format!(
        "{MOCK_CONFIG}\nproject:\n  name: mi-repo\n  base_branch: main\n  branch_prefix: yunta/\n"
    );
    let workflow = r#"
name: project-template
nodes:
  - id: only
    kind: bash
    run: "test '{{project.name}}' = 'mi-repo' && test '{{project.base_branch}}' = 'main' && test '{{project.branch_prefix}}' = 'yunta/'"
"#;
    let RunReport { terminal, state: _ } = bench
        .run_with_config(workflow, "sessions: []", &config)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn an_undefined_inputs_variable_still_fails_the_node_clearly() {
    // Declaring/validating/supplying `{{inputs.*}}` happens elsewhere — absent
    // that, referencing it is exactly the same "undefined variable"
    // failure any other unknown name would get, never silent text.
    let bench = Bench::new();
    let workflow = r#"
name: undefined-input
nodes:
  - id: only
    kind: bash
    run: "echo {{inputs.idea}}"
"#;
    let RunReport { terminal, state: _ } = bench.run(workflow, "sessions: []").await;
    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(reason, "node `only` failed: template references `{{inputs.idea}}`, which is not defined here");
        }
        other => panic!("expected the run to pause citing the undefined variable, got {other:?}"),
    }
}

#[tokio::test]
async fn a_declared_input_s_default_resolves_in_a_node_s_own_template() {
    // `Bench::run` never supplies `--input` values (`&HashMap::new()`
    // throughout its own harness) — an input with a `default` is exactly
    // the case that still has a value to resolve without one.
    let bench = Bench::new();
    let workflow = r#"
name: default-input
inputs:
  greeting:
    type: string
    default: hola
nodes:
  - id: only
    kind: bash
    run: "test '{{inputs.greeting}}' = 'hola'"
"#;
    let RunReport { terminal, state: _ } = bench.run(workflow, "sessions: []").await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn the_stable_and_run_stable_segments_hash_identically_across_runs_with_different_volatile_content(
) {
    // "comparar hashes entre sesiones es la
    // verificación mecánica de que el prefijo se mantuvo estable" —
    // `command:` (volatile) cambia entre las dos corridas; `files:`
    // (stable) y `artifact:` (run-stable) no.
    let bench_a = Bench::new();
    let (stable_a, run_stable_a, segments_a) = run_stable_first(&bench_a, "VOLATILE-A").await;

    let bench_b = Bench::new();
    let (stable_b, run_stable_b, segments_b) = run_stable_first(&bench_b, "VOLATILE-B").await;

    assert_eq!(
        stable_a.content_hash, stable_b.content_hash,
        "the `files:` source itself must hash identically — its own content never changed"
    );
    assert_eq!(run_stable_a.content_hash, run_stable_b.content_hash);

    assert_eq!(
        segments_a["stable"], segments_b["stable"],
        "the stable segment's own canonical text must be byte-identical across runs"
    );
    assert_eq!(segments_a["run-stable"], segments_b["run-stable"]);
    assert_ne!(
        segments_a["volatile"], segments_b["volatile"],
        "the volatile segment must differ when the command's own output differs"
    );
    assert_eq!(
        segments_a.keys().collect::<std::collections::HashSet<_>>(),
        std::collections::HashSet::from([
            &"stable".to_string(),
            &"run-stable".to_string(),
            &"volatile".to_string()
        ]),
        "all three classes are in play for this workflow, so all three must be recorded"
    );
}

// --- knowledge layering, repo > user > org ---------------------

#[tokio::test]
async fn a_knowledge_source_with_only_the_user_layer_resolves_the_user_root_and_is_replayable() {
    let user_home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(user_home.path().join("knowledge")).unwrap();
    std::fs::write(
        user_home.path().join("knowledge/note.md"),
        "MARKER-USER-ONLY-CONTENT\n",
    )
    .unwrap();

    let bench = Bench::new().with_user_state_root(user_home.path());
    let workflow = context_workflow("      - knowledge: { layers: [user] }\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-USER-ONLY-CONTENT\"\n    outcome: { type: completed, summary: ok }\n";

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    assert_eq!(sources[0].kind, "knowledge");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn a_knowledge_source_merges_repo_and_user_with_repo_winning_a_name_collision() {
    let user_home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(user_home.path().join("knowledge")).unwrap();
    // Same filename in both layers: repo must win.
    std::fs::write(
        user_home.path().join("knowledge/shared.md"),
        "MARKER-FROM-USER-LOSES\n",
    )
    .unwrap();
    std::fs::write(
        user_home.path().join("knowledge/user-only.md"),
        "MARKER-USER-ONLY\n",
    )
    .unwrap();

    let bench = Bench::new().with_user_state_root(user_home.path());
    std::fs::create_dir_all(bench.worktree.join(".yunta/knowledge")).unwrap();
    std::fs::write(
        bench.worktree.join(".yunta/knowledge/shared.md"),
        "MARKER-FROM-REPO-WINS\n",
    )
    .unwrap();

    let workflow = context_workflow("      - knowledge: {}\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-FROM-REPO-WINS\"\n    outcome: { type: completed, summary: ok }\n";

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    assert_materialized(&bench.run_dir(), &sources[0]);
    let path = bench
        .run_dir()
        .join("objects")
        .join(sources[0].content_hash.as_str());
    let content = std::fs::read_to_string(path).unwrap();
    assert!(
        content.contains("MARKER-FROM-REPO-WINS"),
        "repo's `shared.md` must win over user's: {content}"
    );
    assert!(
        !content.contains("MARKER-FROM-USER-LOSES"),
        "user's overridden `shared.md` must not survive the merge: {content}"
    );
    assert!(
        content.contains("MARKER-USER-ONLY"),
        "user's own untouched file must still be present: {content}"
    );
}

#[tokio::test]
async fn a_knowledge_source_resolves_an_installed_org_knowledge_pack() {
    // The org layer is the union of installed knowledge packs —
    // "knowledge pack instalado se resuelve
    // como capa org sin config extra".
    let bench = Bench::new();
    write_org_pack(
        &bench.worktree,
        "acme",
        "org-knowledge",
        &[("conventions.md", "MARKER-ORG-CONVENTIONS\n")],
    );

    let workflow = context_workflow("      - knowledge: { layers: [org] }\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-ORG-CONVENTIONS\"\n    outcome: { type: completed, summary: ok }\n";

    let RunReport { terminal, state } = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    assert_eq!(sources[0].kind, "knowledge");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn repo_knowledge_wins_a_name_collision_with_an_org_pack() {
    // The unchanged inter-layer precedence (org < user < repo) now
    // exercised against a real pack — and, since `knowledge: {}` is the
    // default that includes org, this also proves the default source no
    // longer errors the moment a knowledge pack is installed.
    let bench = Bench::new();
    write_org_pack(
        &bench.worktree,
        "acme",
        "org-knowledge",
        &[
            ("shared.md", "MARKER-FROM-ORG-LOSES\n"),
            ("org-only.md", "MARKER-ORG-ONLY\n"),
        ],
    );
    std::fs::create_dir_all(bench.worktree.join(".yunta/knowledge")).unwrap();
    std::fs::write(
        bench.worktree.join(".yunta/knowledge/shared.md"),
        "MARKER-FROM-REPO-WINS\n",
    )
    .unwrap();

    let workflow = context_workflow("      - knowledge: {}\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-FROM-REPO-WINS\"\n    outcome: { type: completed, summary: ok }\n";

    let RunReport { terminal, state } = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    let path = bench
        .run_dir()
        .join("objects")
        .join(sources[0].content_hash.as_str());
    let content = std::fs::read_to_string(path).unwrap();
    assert!(
        content.contains("MARKER-FROM-REPO-WINS"),
        "repo's `shared.md` must win over the org pack's: {content}"
    );
    assert!(
        !content.contains("MARKER-FROM-ORG-LOSES"),
        "the org pack's overridden `shared.md` must not survive the merge: {content}"
    );
    assert!(
        content.contains("MARKER-ORG-ONLY"),
        "the org pack's own untouched file must still be present: {content}"
    );
}

#[tokio::test]
async fn two_org_packs_shipping_the_same_filename_fail_the_node_naming_both() {
    // Between org packs there is no order — same filename from
    // two installed packs is a typed error naming both and the file,
    // never resolved alphabetically or by install order.
    let bench = Bench::new();
    write_org_pack(
        &bench.worktree,
        "acme",
        "pack-a",
        &[("conventions.md", "from pack-a\n")],
    );
    write_org_pack(
        &bench.worktree,
        "globex",
        "pack-b",
        &[("conventions.md", "from pack-b\n")],
    );

    let workflow = context_workflow("      - knowledge: { layers: [org] }\n");
    let fixture = "sessions: []";

    let RunReport { terminal, state } = bench.run(&workflow, fixture).await;
    match &state.nodes.state("ask") {
        Some(yunta_engine::NodeState::Failed { failure, .. }) => {
            assert_eq!(failure.to_string(), "context `knowledge:org` on node `ask`: knowledge file `conventions.md` is shipped by two installed packs — `acme/pack-a` and `globex/pack-b` — and the org layer has no precedence between packs; remove one, or shadow the file with the repo's own `.yunta/knowledge/conventions.md`");
        }
        other => panic!("expected `ask` to fail naming both packs, got {other:?}"),
    }
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the run to pause, got {other:?}"),
    }
}

#[tokio::test]
async fn layers_repo_only_never_mounts_an_installed_org_pack() {
    let bench = Bench::new();
    write_org_pack(
        &bench.worktree,
        "acme",
        "org-knowledge",
        &[("conventions.md", "MARKER-ORG-MUST-NOT-APPEAR\n")],
    );
    std::fs::create_dir_all(bench.worktree.join(".yunta/knowledge")).unwrap();
    std::fs::write(
        bench.worktree.join(".yunta/knowledge/local.md"),
        "MARKER-REPO-LOCAL\n",
    )
    .unwrap();

    let workflow = context_workflow("      - knowledge: { layers: [repo] }\n");
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-REPO-LOCAL\"\n    outcome: { type: completed, summary: ok }\n";

    let RunReport { terminal, state } = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "ask");
    let path = bench
        .run_dir()
        .join("objects")
        .join(sources[0].content_hash.as_str());
    let content = std::fs::read_to_string(path).unwrap();
    assert!(
        !content.contains("MARKER-ORG-MUST-NOT-APPEAR"),
        "`layers: [repo]` must not mount the org pack: {content}"
    );
}

#[tokio::test]
async fn an_org_layer_with_no_packs_installed_resolves_empty_not_an_error() {
    // With a real resolver behind it, an empty org layer is a
    // true answer — same as `user` with no `~/.yunta/knowledge` — not
    // the degradation-with-error refusal a stub without a resolver would give.
    let bench = Bench::new();
    let workflow = context_workflow("      - knowledge: { layers: [org] }\n");
    let fixture = "sessions:\n  - outcome: { type: completed, summary: ok }\n";

    let RunReport { terminal, state } = bench.run(&workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");
}

// --- `context:` at loop level ----------------------------------------

#[tokio::test]
async fn a_loop_s_context_reaches_every_task_s_brief() {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("notes.md"), "the-shared-notes").unwrap();

    let workflow = r#"
name: loop-context
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
    context:
      - files: ["notes.md"]
    prompt: "Read your task from the tasks document and implement it."
"#;
    let tasks = format!(
        "tasks:\n{}{}",
        task_yaml("task-1", "one", "one.txt", "test -f one.txt"),
        task_yaml("task-2", "two", "two.txt", "test -f two.txt"),
    );

    // The executor sessions only match if their prompt actually carries
    // the context block's content — a brief without it dispatches no
    // session and the run fails, so a Finished terminal IS the proof.
    let mut fixture = plan_session(&tasks);
    for n in 1..=2 {
        let file = if n == 1 { "one.txt" } else { "two.txt" };
        fixture.push_str(&format!(
            "  - match_prompt_contains: \"the-shared-notes\"\n    effects:\n      - {{ path: {file}, content: \"x\" }}\n    outcome: {{ type: completed, summary: did-{n} }}\n",
        ));
    }

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    // One `context_assembled` per task brief, each naming its task.
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let assembled_tasks: Vec<String> = events
        .iter()
        .filter(|e| {
            e.node_id
                .as_ref()
                .is_some_and(|id| id.as_str() == "implement")
        })
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Node(NodeEvent::ContextAssembled(p))) => Some(
                p.task_id
                    .as_ref()
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
            ),
            _ => None,
        })
        .collect();
    let mut sorted = assembled_tasks.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec!["task-1".to_string(), "task-2".to_string()],
        "one context_assembled per task, each carrying its task_id: {assembled_tasks:?}"
    );
}

// --- the shape of a declared artifact reaches the session -----------------
//
// A node that declared `kind: tasks` has already said everything
// needed to publish the shape. These prove it arrives without the author
// asking, that it names what carries the document, and that an opaque
// artifact — which has no shape to demand — mounts nothing.

#[tokio::test]
async fn a_node_that_declares_an_interpreted_artifact_is_told_its_shape() {
    let bench = Bench::new();
    let workflow = r#"
name: shape-published
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write a tasks document."
    artifacts:
      produces: [tasks]
"#;
    // The script only matches a prompt carrying the published shape, so
    // the run reaching a session at all is the assertion. `type: guard`
    // appears only in the shape, never in the author's prompt.
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - match_prompt_contains: "type: guard"
    steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            tasks: []
    outcome: { type: completed, summary: planned }
"#;

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let sources = context_sources(&events, "plan");
    assert_eq!(
        sources.len(),
        1,
        "the shape is an audited source like any other"
    );
    assert_eq!(sources[0].kind, "artifact-shape");
    assert_materialized(&bench.run_dir(), &sources[0]);
}

#[tokio::test]
async fn the_shape_tells_the_session_the_run_tools_carry_the_document() {
    let bench = Bench::new();
    let workflow = r#"
name: shape-carrier
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write a tasks document."
    artifacts:
      produces: [tasks]
"#;
    // The file is the engine's to write, so the shape names the way in
    // rather than a path: the script only matches a prompt that says so.
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - match_prompt_contains: "hand it over with `yunta_submit_tasks`"
    steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            tasks: []
    outcome: { type: completed, summary: planned }
"#;

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(workflow, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn an_opaque_artifact_mounts_no_shape_because_it_has_none_to_demand() {
    let bench = Bench::new();
    let workflow = r#"
name: no-shape
nodes:
  - id: write
    kind: prompt
    runner: executor
    prompt: "Write the report."
    artifacts:
      produces: [report.md]
"#;
    let fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/report.md\", content: \"done\\n\" }}\n    outcome: {{ type: completed, summary: written }}\n",
        bench.staging("write").display()
    );

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let assembled_anything = events.iter().any(|e| {
        e.node_id.as_ref().is_some_and(|id| id.as_str() == "write")
            && matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::Node(
                    NodeEvent::ContextAssembled(_)
                ))
            )
    });
    assert!(
        !assembled_anything,
        "an opaque artifact has no shape, so nothing is mounted at all"
    );
}

#[tokio::test]
async fn an_artifact_reference_reaches_only_the_node_it_names() {
    // Two nodes, one name. The reference names `bare`, which produced
    // nothing — and `alpha`'s file of the same name is not an answer to
    // a question about `bare`.
    let bench = Bench::new();
    let workflow = r#"
name: ctx-wrong-producer
nodes:
  - id: alpha
    kind: prompt
    runner: executor
    prompt: "Write the report."
    artifacts:
      produces: [report.md]
  - id: bare
    kind: bash
    depends_on: [alpha]
    run: "true"
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Plan from the report."
    context:
      - artifact: { node: bare, name: report.md }
"#;
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{dir}/report.md", content: "ALPHA-REPORT" }}
    outcome: {{ type: completed, summary: reported }}
  - outcome: {{ type: completed, summary: planned }}
"#,
        dir = bench.staging("alpha").display()
    );

    let RunReport { terminal, state } = bench.run(workflow, &fixture).await;
    match state.nodes.state("plan") {
        Some(yunta_engine::NodeState::Failed { failure, .. }) => {
            let text = failure.to_string();
            assert!(
                text.contains("report.md") && text.contains("bare"),
                "the failure names the artifact and the node that did not produce it: {text}"
            );
        }
        other => {
            panic!("expected plan to fail on an artifact `bare` never produced, got {other:?}")
        }
    }
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
}

#[tokio::test]
async fn an_artifact_reference_without_a_node_resolves_the_last_acceptance_not_the_file() {
    // Two producers of one name, then the view is deleted outright. What
    // the reference resolves is the acceptance standing last on the log,
    // and its bytes come from the store — the directory is not the
    // answer to anything.
    let bench = Bench::new();
    // The view belongs to the engine, so the node that deletes it names
    // it by its absolute path rather than through a template no workflow
    // has for it.
    let workflow = format!(
        r#"
name: ctx-latest-acceptance
nodes:
  - id: alpha
    kind: prompt
    runner: executor
    prompt: "Write the first report."
    artifacts:
      produces: [report.md]
  - id: beta
    kind: prompt
    runner: executor
    depends_on: [alpha]
    prompt: "Write the second report."
    artifacts:
      produces: [report.md]
  - id: wipe
    kind: bash
    depends_on: [beta]
    run: "rm -rf {view}"
  - id: plan
    kind: prompt
    runner: executor
    depends_on: [wipe]
    prompt: "Plan from the report."
    context:
      - artifact: {{ name: report.md }}
"#,
        view = bench.run_dir().join(yunta_core::ARTIFACTS_DIR).display()
    );
    let fixture = format!(
        r#"
sessions:
  - effects:
      - {{ path: "{alpha}/report.md", content: "ALPHA-REPORT" }}
    outcome: {{ type: completed, summary: first }}
  - effects:
      - {{ path: "{beta}/report.md", content: "BETA-REPORT" }}
    outcome: {{ type: completed, summary: second }}
  - match_prompt_contains: "BETA-REPORT"
    outcome: {{ type: completed, summary: planned }}
"#,
        alpha = bench.staging("alpha").display(),
        beta = bench.staging("beta").display()
    );

    let RunReport { terminal, state } = bench.run(&workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
}
