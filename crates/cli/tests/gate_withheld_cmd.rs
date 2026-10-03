//! A gate that withholds going on, as a person meets it: the menu offers
//! only what sends the plan back or stops the run, what keeps the plan
//! from being proven is said between the question and the options, and
//! the option it withholds is refused with why wherever it is chosen.

use yunta_testkit::{run_id_from, stderr, stdout, yunta_in, yunta_on_terminal, Checkout, Terminal};

/// The planner, the node that specifies its tasks, and the gate that
/// shows both.
const GOVERNED: &str = r#"
name: governed
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: spec
    kind: prompt
    runner: planner
    permissions: read-only
    depends_on: [plan]
    prompt: "Write the tests the plan's tasks are held to."
    artifacts:
      produces: [spec]
  - id: approve-plan
    kind: gate
    depends_on: [spec]
    assignee: lead
    message: "Plan and its tests registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows:
      - { node: plan, kind: tasks }
      - { node: spec, kind: spec }
"#;

/// A plan that changes `tests/greet.sh`, and the spec that then writes
/// that file as the task's test.
const SESSIONS: &str = r#"
capabilities: { run_tools: true }
sessions:
  - match_prompt_contains: "Hand over the tasks"
    steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Greet"
            description: "Writes the greeting the run is about."
            risks: ["A greeting file left half written reads as no greeting at all. It is written whole."]
            tasks:
              - { id: greet, title: "Write the greeting", description: "Writes greeting.txt, and a check of it.", scope: [greeting.txt, tests/greet.sh], changes: [{ at: greeting.txt, what: "the greeting", code: "Hello" }, { at: tests/greet.sh, what: "a check of the greeting", code: "test -s greeting.txt" }], outcome: "greeting.txt says hello", criteria: [{ cmd: "test \"$(cat greeting.txt 2>/dev/null)\" = Hello", proves: "the greeting says hello" }] }
    outcome: { type: completed, summary: planned }
  - match_prompt_contains: "Write the tests"
    steps:
      - type: run_tool
        tool: yunta_submit_spec
        arguments:
          document:
            specs:
              - { task: greet, files: [{ path: tests/greet.sh, content: "test \"$(cat greeting.txt 2>/dev/null)\" = Hello\n" }], tests: [{ cmd: "sh tests/greet.sh", proves: "the greeting says hello" }] }
    outcome: { type: completed, summary: specified }
"#;

const RUN: [&str; 7] = [
    "run",
    "wf.yaml",
    "--adapter",
    "mock",
    "--fixture",
    "fixture.yaml",
    "--quiet",
];

fn checkout(root: &std::path::Path) -> Checkout {
    Checkout::under(root)
        .config(
            "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
             - { adapter: mock, model: mock-model }\n",
        )
        .workflow("wf", GOVERNED)
        .file("fixture.yaml", SESSIONS)
        .committed()
}

/// The run parked on its gate, off a terminal, and its id.
fn parked(checkout: &Checkout) -> String {
    let parked = yunta_in!(&checkout.repo, &checkout.home, &RUN);
    assert_eq!(parked.status.code(), Some(3), "{}", stderr(&parked));
    run_id_from(&parked)
}

#[test]
fn a_gate_that_withholds_says_why_between_its_question_and_what_is_left() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN);
    terminal.wait_for("2  abort", "the gate never put its options on the console");
    let drawn = terminal.drawn();

    assert!(
        drawn.contains("1  adjust") && !drawn.contains("  approve"),
        "the menu offers only what sends the plan back or stops the run:\n{drawn}"
    );
    let question = drawn
        .rfind("decision: ◆ needs you")
        .expect("the decision was put");
    let before = &drawn[question..];
    assert!(
        before.contains("before you decide")
            && before.contains("`approve` is not offered, because")
            && before.contains("`greet` plans to change `tests/greet.sh`")
            && before.contains("risk: A greeting file left half written reads as no"),
        "what weighs on the decision is said beside its options:\n{drawn}"
    );
    let screen: Vec<&str> = drawn.lines().collect();
    let menu = screen
        .iter()
        .rposition(|line| line.contains("2  abort"))
        .expect("the menu was drawn");
    let last = &screen[(menu + 1).saturating_sub(usize::from(Terminal::ROWS))..=menu];
    assert!(
        last.iter().any(|line| line.contains("before you decide")),
        "what weighs on the decision is on the screen the menu asks on:\n{}",
        last.join("\n")
    );
}

#[test]
fn a_withheld_option_is_refused_with_why_when_it_is_answered_from_elsewhere() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let run_id = parked(&checkout);

    let refused = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &["resolve-gate", &run_id, "approve"]
    );

    assert!(!refused.status.success(), "{}", stdout(&refused));
    let said = stderr(&refused);
    assert!(
        said.contains("`approve` is withheld") && said.contains("`tests/greet.sh`"),
        "{said}"
    );
}

#[test]
fn status_says_what_weighs_on_the_decision_and_what_the_gate_withholds() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let run_id = parked(&checkout);

    let page = yunta_in!(&checkout.repo, &checkout.home, &["status", &run_id]);
    let page = stdout(&page);
    let lines: Vec<&str> = page.lines().collect();
    let first_option = lines
        .iter()
        .position(|line| line.trim_start().starts_with("adjust"))
        .expect(&page);
    assert!(
        lines[first_option - 1].trim().is_empty(),
        "what weighs on the decision stands apart from its options:\n{page}"
    );
    let before = page.find("before you decide").expect(&page);
    let adjust = page.find("adjust").expect(&page);
    assert!(before < page.rfind("adjust").unwrap_or(adjust), "{page}");

    let json = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &["status", &run_id, "--json"]
    );
    let document: serde_json::Value = serde_json::from_str(&stdout(&json)).unwrap();
    let withheld = &document["decision"]["withheld"];
    assert_eq!(withheld[0]["option"], "approve", "{document:#}");
    assert!(
        withheld[0]["because"]
            .as_str()
            .is_some_and(|because| because.contains("`tests/greet.sh`")),
        "{document:#}"
    );
}
