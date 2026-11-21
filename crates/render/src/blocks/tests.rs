use std::path::{Path, PathBuf};

use serde_json::json;
use surco::components::{DecisionOption, Region};
use surco::doc::{Block, Doc};
use surco::golden::{assert_golden, Environment};
use surco::look::Look;
use surco::paint::Paint;
use surco::surface::{Surface, Terminal};
use yunta_core::events::{ArtifactId, ResolvedRunner, StoredEvent};
use yunta_core::{
    AdapterId, ArtifactKind, ArtifactSpec, BaselineConfig, ConfigLayer, ModelName, Node, NodeId,
    RunId, RunnerCandidate, RunnerName, TaskId, Workflow,
};
use yunta_replay::frame::{NodeFrame, NodeStanding};
use yunta_replay::RunState;

use super::{HandsOver, NodeCommand, NodeTitle, TaskProof, TaskScope, WhereItStopped};

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
        let look = Look::of(environment);
        let proof = TaskProof::of(&task, Some(TITLE), &state).expect("a task registered");
        let scope = TaskScope::of(&task, &state).expect("a task registered");
        let blocks = proof
            .blocks(&look)
            .into_iter()
            .chain(scope.blocks(look.glyphs));
        assert_golden(
            &environment.golden(&goldens(), "task"),
            &drawn(blocks.collect(), environment),
        );
    }
}

/// A task at work turns on the spinner's frame of the run's last event in a pane whose look
/// moves; still, or never turned, as a printed page shows it, its row keeps its mark.
#[test]
fn a_task_at_work_turns_where_the_look_moves_and_keeps_its_mark_elsewhere() {
    let state = a_task_red();
    let task = TaskId::from(TASK);
    let proof = || TaskProof::of(&task, Some(TITLE), &state).expect("a task registered");
    let moving = Look {
        motion: surco::motion::Motion::Moves,
        ..Look::plain()
    };
    let opens = |proof: &TaskProof, look: &Look| proof.row(look).text().chars().next();
    assert_eq!(opens(&proof().turning(7), &moving), Some('\\'));
    assert_eq!(opens(&proof().turning(7), &Look::plain()), Some('>'));
    assert_eq!(opens(&proof(), &moving), Some('>'));
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

/// A workflow of each kind a pane titles otherwise: a session's prompt, a check against the
/// baseline, a command the engine runs itself and a gate a person answers.
const EACH_KIND: &str = r#"
name: each-kind
nodes:
  - id: plan
    kind: prompt
    prompt: "plan it"
  - id: tests
    kind: check
    builtin: baseline_compare
    depends_on: [plan]
  - id: build
    kind: bash
    run: "true"
    depends_on: [tests]
  - id: approve
    kind: gate
    assignee: reviewer
    depends_on: [build]
"#;

/// `node` of [`EACH_KIND`], framed as a node that started, with the runner `runner` resolved
/// where one is given.
fn framed_node(workflow: &Workflow, node: &str, runner: Option<&'static str>) -> (NodeFrame, Node) {
    let declared = workflow
        .iter_nodes()
        .find(|declared| declared.id.as_str() == node)
        .expect("a declared node")
        .clone();
    let mut frame = NodeFrame::sample(&NodeId::from(node), NodeStanding::ToGo);
    frame.kind = declared.kind.kind_name();
    frame.runner = runner.map(|runner| ResolvedRunner {
        runner: RunnerName::from_static(runner),
        chosen: RunnerCandidate {
            adapter: AdapterId::from_static("mock"),
            model: "gpt-6-luna".parse::<ModelName>().expect("a model name"),
            agent: None,
            effort: None,
        },
        discarded: Vec::new(),
    });
    (frame, declared)
}

/// A pane is titled by its node and its kind, and at its far end by who works it: a
/// session's runner on its adapter and model, a check's builtin, a gate's assignee, and no
/// one for a command the engine runs itself.
#[test]
fn a_pane_is_titled_by_its_node_its_kind_and_who_works_it() {
    let workflow = Workflow::parse(EACH_KIND).expect("the workflow parses");
    let cases = [
        (
            "plan",
            Some("planner"),
            "prompt",
            Some("planner on mock/gpt-6-luna"),
        ),
        ("tests", None, "check", Some("baseline_compare")),
        ("build", None, "bash", None),
        ("approve", None, "gate", Some("assignee reviewer")),
    ];
    for (node, runner, kind, works_it) in cases {
        let (frame, declared) = framed_node(&workflow, node, runner);
        let (state, config) = (RunState::default(), ConfigLayer::default());
        let title = NodeTitle::of(&frame, Some(&declared), &state, &config);
        let region = title.titled(Region::new(Doc::new()));
        assert_eq!(region.title, [node, kind], "{node}");
        assert_eq!(region.edge.as_deref(), works_it, "{node}");
    }
}

/// Two session nodes that name one role, and a third that names another.
const SHARED_RUNNER: &str = r#"
name: one-role
nodes:
  - id: brief
    kind: prompt
    prompt: "brief it"
    runner: planner
  - id: spec
    kind: prompt
    prompt: "spec it"
    runner: planner
    depends_on: [brief]
  - id: build
    kind: prompt
    prompt: "build it"
    runner: executor
    depends_on: [spec]
"#;

/// A session's node that resolved no runner yet is worked by the one its run resolved for
/// the role it names on another node; a role its run never resolved names no one yet.
#[test]
fn a_node_not_run_yet_is_worked_by_the_runner_its_run_resolved_for_its_role() {
    let workflow = Workflow::parse(SHARED_RUNNER).expect("the workflow parses");
    let state = folded(vec![json!({"kind": "runner_resolved", "node_id": "brief",
        "runner": "planner", "chosen": {"adapter": "mock", "model": "gpt-6-luna"},
        "discarded": []})]);
    let config = ConfigLayer::default();
    for (node, works_it) in [
        ("spec", Some("planner on mock/gpt-6-luna")),
        ("build", None),
    ] {
        let (frame, declared) = framed_node(&workflow, node, None);
        let title = NodeTitle::of(&frame, Some(&declared), &state, &config);
        let region = title.titled(Region::new(Doc::new()));
        assert_eq!(region.edge.as_deref(), works_it, "{node}");
    }
}

/// A command node shows its own command, and a check against the baseline the suite its
/// run's lineage measured, which the frozen configuration says before a measurement does;
/// a session's node runs no command of its own.
#[test]
fn what_a_node_runs_is_its_command_or_the_suite_its_check_compares() {
    let workflow = Workflow::parse(EACH_KIND).expect("the workflow parses");
    let config = ConfigLayer {
        baseline: Some(BaselineConfig {
            suite: "cargo test".to_string(),
            held: Vec::new(),
        }),
        ..ConfigLayer::default()
    };
    let unmeasured = folded(Vec::new());
    let measured = folded(vec![json!({"kind": "baseline_captured",
        "command": "cargo test --workspace", "results": {"exit_code": 0, "summary": "ok"},
        "hash": "040f".repeat(16), "origin": {"type": "measured"}})]);
    let runs = |node: &str, state: &RunState| {
        let (_, declared) = framed_node(&workflow, node, None);
        NodeCommand::of(&declared, &config, state).map(|command| {
            let look = Look::of(&IN_A_PANE[1]);
            Terminal::on(look).draw(&Doc::new().with_all(command.blocks()))
        })
    };
    assert_eq!(
        runs("build", &unmeasured).as_deref(),
        Some("what it runs\ntrue\n")
    );
    assert_eq!(
        runs("tests", &unmeasured).as_deref(),
        Some("what it runs\ncargo test\n")
    );
    assert_eq!(
        runs("tests", &measured).as_deref(),
        Some("what it runs\ncargo test --workspace\n")
    );
    assert_eq!(runs("plan", &measured), None);
}

/// A run standing still for room says what it waits on, then what is free against what is
/// needed, their labels in one column two cells past the widest; short of rows, what it
/// waits on gives way whole and the two figures stay. A stop says why under the same rule.
#[test]
fn a_standstill_says_what_it_waits_on_then_its_facts_on_labelled_rows() {
    let look = Look::of(&IN_A_PANE[1]);
    let terminal = Terminal::on(look);
    let no_room = WhereItStopped::NoRoom {
        waits_on: "no room on the disk".to_string(),
        free: 42_000_000_000,
        needed: 9_000_000_000_000_000,
    };
    assert_eq!(
        terminal.draw(&Doc::new().with_all(no_room.blocks(look.glyphs))),
        "what it waits on\nno room on the disk\n\nfree    42 GB\nneeded  9000 TB\n"
    );
    assert_eq!(
        terminal.draw(&Doc::new().with_all(no_room.least(look.glyphs))),
        "free    42 GB\nneeded  9000 TB\n"
    );
    let stopped = WhereItStopped::Stopped {
        held: Vec::new(),
        why: "interrupted by a person".to_string(),
    };
    assert_eq!(
        terminal.draw(&Doc::new().with_all(stopped.blocks(look.glyphs))),
        "where it stopped\nwhy  interrupted by a person\n"
    );
}
