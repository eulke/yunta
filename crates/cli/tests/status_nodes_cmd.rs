//! `yunta status` on the nodes of a run: which ones it lists, in what
//! order, and what it says each of them is doing.
//!
//! One derivation for the whole page (D179): the frame the live view
//! draws is the frame `status` prints and the frame `--json` carries,
//! so the three never disagree about what a run has or where a node
//! sits.

use yunta_testkit::{run_id_from, stderr, stdout, Checkout};

/// Nodes out of alphabetical order, a `parallel` group with children,
/// and a mode that leaves one out — everything the list has to get
/// right in one workflow.
const SHAPED: &str = r#"
name: shaped
modes:
  quick: { include: [zeta, review, alpha] }
  full: { include: all }
nodes:
  - id: zeta
    kind: bash
    run: "true"
  - id: review
    kind: parallel
    depends_on: [zeta]
    nodes:
      - { id: review-a, kind: bash, run: "true" }
      - { id: review-b, kind: bash, run: "true" }
  - id: alpha
    kind: bash
    depends_on: [review]
    run: "true"
  - id: omega
    kind: bash
    depends_on: [alpha]
    run: "true"
"#;

fn shaped_run() -> (Checkout, String) {
    let project = Checkout::new()
        .working_in_place()
        .workflow("wf", SHAPED)
        .committed();
    let run = project.run(
        std::path::Path::new(env!("CARGO_BIN_EXE_yunta")),
        &["run", "wf.yaml", "--mode", "quick"],
    );
    let run_id = run_id_from(&run);
    (project, run_id)
}

fn status(project: &Checkout, run_id: &str, extra: &[&str]) -> String {
    let mut args = vec!["status", run_id];
    args.extend_from_slice(extra);
    stdout(&project.run(std::path::Path::new(env!("CARGO_BIN_EXE_yunta")), &args))
}

/// Declaration order, not alphabetical; every declared node, not only
/// the ones the log named; and a group's children one step under it.
#[test]
fn status_lists_every_declared_node_in_declaration_order_with_children_under_their_group() {
    let (project, run_id) = shaped_run();
    let text = status(&project, &run_id, &[]);
    let listed: Vec<&str> = text
        .lines()
        .skip_while(|line| *line != "nodes:")
        .skip(1)
        .take_while(|line| line.starts_with(' '))
        .collect();

    assert_eq!(
        listed
            .iter()
            .map(|line| line.trim().split(':').next().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["zeta", "review", "review-a", "review-b", "alpha", "omega"],
        "the frame's own order, groups with their children: {text}"
    );
    assert!(
        listed[2].starts_with("    review-a") && listed[1].starts_with("  review"),
        "a group's children sit one step under it: {text}"
    );
    assert!(
        listed[5].contains("skipped"),
        "a node this mode leaves out is listed and said to be left out: {text}"
    );
}

/// The `--json` document carries the same list, in the same order,
/// under the schema version that says its shape changed.
#[test]
fn status_json_lists_every_declared_node_in_order_under_schema_version_five() {
    let (project, run_id) = shaped_run();
    let document: serde_json::Value =
        serde_json::from_str(&status(&project, &run_id, &["--json"])).expect("a JSON document");

    assert_eq!(document["schema_version"], 5);
    let nodes = document["nodes"].as_array().expect("a list of nodes");
    assert_eq!(
        nodes
            .iter()
            .map(|node| node["id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        ["zeta", "review", "review-a", "review-b", "alpha", "omega"]
    );
    assert_eq!(nodes[2]["group"], "review", "{document:#}");
    assert_eq!(nodes[5]["state"], "skipped", "{document:#}");
    assert!(
        nodes[0]["group"].is_null(),
        "a top-level node belongs to no group: {document:#}"
    );
}

/// The workflow and the scripted session behind the questions tests: a
/// node that asks, and a session that submits one question and closes.
const ASKING: &str = r#"
name: asking
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: "Ask what has to be known before going on."
    artifacts:
      produces: [questions]
"#;

const ASKING_FIXTURE: &str = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - id: summary
                text: "What changed?"
                answer_type: text
                required: true
    outcome: { type: completed, summary: asked }
"#;

/// A run of [`ASKING`], parked on the question its session asked, with
/// the moments the run wrote as it went.
fn asking_run() -> (Checkout, String, String) {
    let project = Checkout::new()
        .config(
            "defaults:\n  isolation: none\nrunners:\n  executor:\n    - { adapter: mock, \
             model: mock-model }\n",
        )
        .workflow("wf", ASKING)
        .file("fixture.yaml", ASKING_FIXTURE)
        .committed();
    let run = project.run(
        std::path::Path::new(env!("CARGO_BIN_EXE_yunta")),
        &[
            "run",
            "wf.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml",
        ],
    );
    let run_id = run_id_from(&run);
    let moments = stderr(&run);
    (project, run_id, moments)
}

/// A node parked on its own questions says which ones, and says it the
/// same way wherever a reader meets it.
#[test]
fn status_says_which_questions_a_node_is_waiting_on() {
    let (project, run_id, _) = asking_run();

    let text = status(&project, &run_id, &[]);
    assert!(
        text.contains("ask: waiting — asked 1 question: `summary`"),
        "the node's own line says what it asked: {text}"
    );

    let document: serde_json::Value =
        serde_json::from_str(&status(&project, &run_id, &["--json"])).expect("a JSON document");
    let node = document["nodes"]
        .as_array()
        .and_then(|nodes| nodes.first())
        .expect("the one node");
    assert_eq!(node["waiting_on"]["on"], "questions", "{document:#}");
    assert_eq!(node["waiting_on"]["asked"][0], "summary", "{document:#}");
    assert_eq!(
        node["detail"], "asked 1 question: `summary`",
        "{document:#}"
    );
}

/// The list `status` prints and the list `--json` carries are the same
/// frame read twice: same ids, same order, no node in one and not the
/// other.
#[test]
fn status_lists_the_nodes_the_frame_declares_in_its_order() {
    let (project, run_id) = shaped_run();

    let printed: Vec<String> = status(&project, &run_id, &[])
        .lines()
        .skip_while(|line| *line != "nodes:")
        .skip(1)
        .take_while(|line| line.starts_with(' '))
        .map(|line| {
            line.trim()
                .split(':')
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect();

    let document: serde_json::Value =
        serde_json::from_str(&status(&project, &run_id, &["--json"])).expect("a JSON document");
    let carried: Vec<String> = document["nodes"]
        .as_array()
        .expect("a list of nodes")
        .iter()
        .map(|node| node["id"].as_str().unwrap_or_default().to_string())
        .collect();

    assert_eq!(printed, carried, "{document:#}");
}

/// The chronicle says a node asked in the same words the page says it
/// is waiting on: one modifier, built once, wherever a reader meets it.
#[test]
fn the_chronicle_and_status_say_a_node_that_asked_with_the_same_bytes() {
    let (project, run_id, moments) = asking_run();
    let said = "asked 1 question: `summary`";

    assert!(
        moments.contains(said),
        "the chronicle says what the node asked as it happens: {moments}"
    );
    assert!(
        status(&project, &run_id, &[]).contains(said),
        "and the page says it the same way afterwards: {moments}"
    );
}
