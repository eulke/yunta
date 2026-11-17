use std::path::{Path, PathBuf};

use serde_json::json;
use surco::components::DecisionOption;
use surco::doc::{Block, Doc};
use surco::golden::{assert_golden, Environment};
use surco::look::Look;
use surco::paint::Paint;
use surco::surface::{Surface, Terminal};
use yunta_core::events::{ArtifactId, StoredEvent};
use yunta_core::{ArtifactKind, ArtifactSpec, NodeId, RunId, TaskId};
use yunta_replay::RunState;

use super::{HandsOver, TaskProof, TaskScope};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/blocks")
}

#[test]
fn a_decision_prints_one_command_per_option_with_the_handle() {
    let options = vec![
        DecisionOption {
            id: "retry".to_string(),
            label: Some("runs `fix-lint` once more".to_string()),
            tradeoff: Some("one more correction attempt beyond `max_reroutes`".to_string()),
            asks: None,
        },
        DecisionOption {
            id: "adjust".to_string(),
            label: None,
            tradeoff: Some("the run goes on with what you say".to_string()),
            asks: Some("what to change".to_string()),
        },
    ];
    let decision = super::decision::here(&RunId::from_static("7E5PH4"), options);
    let look = Look::plain();
    let drawn: Vec<String> = surco::surface::Terminal::on(look)
        .block(&decision.into())
        .iter()
        .map(|line| look.paint.line(line))
        .collect();
    let commands: Vec<&String> = drawn
        .iter()
        .filter(|line| line.contains("yunta resolve-gate"))
        .collect();
    assert_eq!(
        commands,
        [
            "|   yunta resolve-gate 7E5PH4 retry",
            "|   yunta resolve-gate 7E5PH4 adjust --text \"<answer>\""
        ]
    );
    assert!(
        drawn
            .iter()
            .any(|line| line.contains("asks: what to change")),
        "{drawn:?}"
    );
}

#[test]
fn a_quoted_refusal_matches_its_golden() {
    let text = "name: fix\nnodes:\n  - id: lint\n    kind: bash\n    run: { command: lnt }\n  - id: fix\n    kind: prompt\n    prompt: p\n    runner: implementr\n";
    let at = |line, col, len| Some(yunta_core::yaml::Location { line, col, len });
    let problems = vec![
        (
            "node `lint`: it runs the project's command `lnt`, and the config declares none",
            at(5, 21, 3),
            Some("lint"),
        ),
        (
            "node `fix` references runner `implementr`, which `runners:` does not define",
            at(9, 13, 10),
            Some("implementer"),
        ),
        ("a conflict between config layers", None, None),
    ];
    assert_golden(
        &goldens().join("quoted-refusal.txt"),
        &super::diagnostic::plain(
            "wf.yaml",
            problems
                .into_iter()
                .map(|(said, at, near)| {
                    super::diagnostic::located(said.to_string(), at, "wf.yaml", Some(text))
                        .help(near.map(surco::text::suggested))
                })
                .collect(),
        ),
    );
}

/// The cells of a run's pane at 120 columns, where a node's blocks stand beside its nodes.
const PANE: usize = 84;

/// A run's pane at 120 columns: on the 16-color layer, plain, and in ASCII.
const IN_A_PANE: [Environment; 3] = [
    Environment {
        name: "unicode-color-84",
        unicode: true,
        paint: Paint::ANSI16,
        width: PANE,
    },
    Environment {
        name: "unicode-84",
        unicode: true,
        paint: Paint::Plain,
        width: PANE,
    },
    Environment {
        name: "ascii-84",
        unicode: false,
        paint: Paint::Plain,
        width: PANE,
    },
];

/// The task every case works.
const TASK: &str = "global-pack-install";

/// What proves it: the one criterion its plan declares.
const CRITERION: &str = "cargo test --workspace --test pack_global_spec \
    global_pack_commands_use_the_per_user_store_without_changing_the_project_store";

/// Its title, as the tasks document gives it.
const TITLE: &str = "Add explicit global pack install target";

/// The run `lines` state, each a logged event without its run, place and time, folded in
/// order.
fn folded(lines: Vec<serde_json::Value>) -> RunState {
    let events: Vec<StoredEvent> = lines
        .into_iter()
        .enumerate()
        .map(|(at, mut line)| {
            line["run_id"] = json!("01M7DES1GNED00000000Q78GRR");
            line["seq"] = json!(at + 1);
            line["timestamp"] = json!("2026-01-01T00:00:00Z");
            serde_json::from_value(line).expect("a logged event")
        })
        .collect();
    RunState::replay(&events)
}

/// `blocks` drawn in `environment` as a document holds them.
fn drawn(blocks: Vec<Block>, environment: &Environment) -> String {
    let look = Look::of(environment);
    Terminal::on(look).draw(&Doc::new().with_all(blocks))
}

/// `implement`'s session for the task wrote two of the three paths it declared, a person
/// granted it a fourth, and its check after the work left its criterion red, in a run whose
/// lint was green at the baseline.
fn a_task_red() -> RunState {
    let edited = |path: &str| {
        json!({"kind": "agent_message", "node_id": "implement", "message_type": "tool_use",
            "action": "edit", "tool_name": "Edit", "session": "mock-session-5",
            "target": {"display": path, "digest": "855e".repeat(16)}})
    };
    folded(vec![
        json!({"kind": "baseline_held", "name": "lint",
            "command": "cargo clippy --workspace --all-targets", "origin": {"type": "measured"},
            "results": {"exit_code": 0, "summary": "Finished"}}),
        json!({"kind": "task_registered", "node_id": "plan", "task_id": TASK,
            "criteria": [{"cmd": CRITERION}],
            "scope": ["crates/cli/src/pack.rs", "crates/cli/src/cli/pack.rs",
                "crates/cli/tests/pack_cmd.rs"]}),
        json!({"kind": "node_started", "node_id": "implement", "attempt": 1}),
        json!({"kind": "task_status_changed", "node_id": "implement", "task_id": TASK,
            "new_status": "running", "caused_by": 2}),
        json!({"kind": "agent_session_opened", "node_id": "implement",
            "session_id": "mock-session-5", "task_id": TASK, "capabilities": {}}),
        edited("crates/cli/src/pack.rs"),
        edited("crates/cli/src/cli/pack.rs"),
        json!({"kind": "criteria_checked", "node_id": "implement", "task_id": TASK,
            "phase": "post", "results": [{"cmd": CRITERION, "exit_code": 101, "reused": false,
                "tail": ["test result: FAILED. 0 passed; 1 failed"]}]}),
        json!({"kind": "scope_expansion_granted", "node_id": "implement", "task_id": TASK,
            "decided_by": {"type": "person", "id": "mcp"}, "mode": "ask", "count_this_run": 0,
            "paths": ["crates/cli/src/commands/pack_audit.rs"]}),
    ])
}

/// A task at work says its title and attempt, that its proof is red and by which command,
/// what the run holds it to, and its scope a path a row, marked where its session wrote one,
/// with the path a person granted it.
#[test]
fn a_task_red_by_its_command_with_two_of_three_paths_written_matches_its_goldens() {
    let state = a_task_red();
    let task = TaskId::from(TASK);
    for environment in &IN_A_PANE {
        let glyphs = Look::of(environment).glyphs;
        let proof = TaskProof::of(&task, Some(TITLE), &state).expect("a task registered");
        let scope = TaskScope::of(&task, &state).expect("a task registered");
        let blocks = proof.blocks(glyphs).into_iter().chain(scope.blocks(glyphs));
        assert_golden(
            &environment.golden(&goldens(), "task"),
            &drawn(blocks.collect(), environment),
        );
    }
}

/// Bytes a spec no longer reads as: its specs are no list.
const SPECS_NO_LIST: &[u8] = b"specs: 7\n";

/// Where a run's view keeps the spec, from the run's own folder.
const SPEC_VIEW: &str = "artifacts/spec/spec.yaml";

/// `plan` declares a plan and a spec, and the engine accepted both on its attempt; the
/// spec cannot be read as today's shape reads it.
fn a_plan_and_a_spec_accepted() -> RunState {
    let accepted = |kind: &str| {
        json!({"kind": "artifact_accepted", "node_id": "plan", "origin": {"kind": "submitted"},
            "artifact": {"type": "interpreted", "kind": kind}, "content_hash": "c0ba".repeat(16)})
    };
    folded(vec![
        json!({"kind": "node_started", "node_id": "plan", "attempt": 1}),
        accepted("tasks"),
        accepted("spec"),
    ])
}

/// What a node declares it hands over is said a document a row, with where it stands: the one
/// the engine read and accepted, and the one it accepted that cannot be read, in its own
/// errors as its shape refuses it: its path and count, then each error.
#[test]
fn a_node_that_handed_one_document_accepted_and_one_unread_matches_its_goldens() {
    let state = a_plan_and_a_spec_accepted();
    let declared = [
        ArtifactSpec::Interpreted(ArtifactKind::Tasks),
        ArtifactSpec::Interpreted(ArtifactKind::Spec),
    ];
    let refused = yunta_core::shape::read::<yunta_core::SpecFile>(SPECS_NO_LIST, SPEC_VIEW)
        .expect_err("specs that are no list fail the shape");
    let unread = [(
        ArtifactId::Interpreted {
            kind: ArtifactKind::Spec,
        },
        refused.to_string(),
    )];
    let handed = HandsOver::of(&NodeId::from("plan"), &declared, &state, &unread);
    for environment in &IN_A_PANE {
        let look = Look::of(environment);
        assert_golden(
            &environment.golden(&goldens(), "hands-over"),
            &drawn(handed.blocks(&look), environment),
        );
    }
}

/// A document is handed over by the attempt that is open: the plan an attempt sent back
/// handed over is not yet handed over by the next one.
#[test]
fn a_plan_handed_over_by_an_attempt_sent_back_is_not_yet_handed_over_by_the_next() {
    let state = folded(vec![
        json!({"kind": "node_started", "node_id": "plan", "attempt": 1}),
        json!({"kind": "artifact_accepted", "node_id": "plan", "origin": {"kind": "submitted"},
            "artifact": {"type": "interpreted", "kind": "tasks"},
            "content_hash": "c0ba".repeat(16)}),
        json!({"kind": "node_finished", "node_id": "plan", "outcome": "planned",
            "tokens_used": {"input": 0, "output": 0}}),
        json!({"kind": "node_started", "node_id": "plan", "attempt": 2}),
    ]);
    let declared = [ArtifactSpec::Interpreted(ArtifactKind::Tasks)];
    let handed = HandsOver::of(&NodeId::from("plan"), &declared, &state, &[]);
    for environment in &IN_A_PANE {
        let look = Look::of(environment);
        assert_golden(
            &environment.golden(&goldens(), "hands-over-not-yet"),
            &drawn(handed.blocks(&look), environment),
        );
    }
}
