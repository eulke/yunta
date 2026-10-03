//! A plan put in front of the person approving it, driven end to end on a
//! terminal: the gate draws the tasks it asks about, a correction without
//! words is asked again, and one with words sends the plan back to the
//! planner before the gate asks once more.

use yunta_testkit::{run_id_from, stderr, yunta_in, yunta_on_terminal, Checkout, Terminal};

const REVIEWED_PLAN: &str = r#"
name: reviewed-plan
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    message: "Plan registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows: [{ node: plan, kind: tasks }]
"#;

/// The first plan, and the one after the correction.
const PLANNER: &str = r#"
capabilities: { run_tools: true, resume_session: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Make it"
            description: "Writes the file the run is about."
            tasks:
              - { id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], changes: [{ at: made.txt, what: "the file", code: "the file's first line" }], outcome: "made.txt exists", criteria: [{ cmd: "test -f made.txt", proves: "the file exists" }] }
    outcome: { type: completed, summary: "planned" }
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Make it"
            description: "Writes the file the run is about."
            tasks:
              - { id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], changes: [{ at: made.txt, what: "the file", code: "the file's first line" }], outcome: "made.txt exists", criteria: [{ cmd: "test -f made.txt", proves: "the file exists" }] }
              - { id: T002, title: "Say it", description: "Writes said.txt.", scope: [said.txt], changes: [{ at: said.txt, what: "the file", code: "the file's first line" }], outcome: "said.txt exists", criteria: [{ cmd: "test -f said.txt", proves: "it was said" }] }
    outcome: { type: completed, summary: "planned again" }
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
        .workflow("wf", REVIEWED_PLAN)
        .file("fixture.yaml", PLANNER)
        .committed()
}

fn reviewing(root: &std::path::Path) -> Terminal {
    let checkout = checkout(root);
    yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN)
}

#[test]
fn a_plan_is_reviewed_at_its_gate_and_a_correction_sends_it_back_with_the_words() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = reviewing(root.path());
    terminal.wait_for("3  abort", "the gate never put its options on the console");
    let drawn = terminal.drawn();
    assert!(
        drawn.contains("plan of `plan`")
            && drawn.contains("Writes the file the run is about.")
            && drawn.contains("T001 — Make it")
            && drawn.contains("done when    the file exists")
            && drawn.contains("$ test -f made.txt")
            && drawn.contains("--node plan"),
        "the gate shows the plan it asks about, as the run judges it:\n{drawn}"
    );

    // `adjust`, the second option.
    terminal.keys("\x1b[B\r");
    terminal.wait_for(
        "what should change?",
        "the correction never asked for words",
    );
    terminal.keys("\r");
    terminal.wait_for(
        "this option needs an answer",
        "an empty correction was taken as an answer",
    );
    terminal.keys("add a task that says it\r");

    // The planner goes again and the gate asks about the second plan.
    terminal.wait_for(
        "T002 — Say it",
        "the gate never showed the plan the correction made",
    );
    // `approve`, the first option, with nothing to add.
    terminal.keys("\r");
    terminal.wait_for(
        "enter records it, n adds a note",
        "approving asked for words it does not need",
    );
    terminal.keys("\r");

    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
}

#[test]
fn an_option_that_asks_requires_its_words() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = reviewing(root.path());
    terminal.wait_for("3  abort", "the gate never put its options on the console");
    // `adjust` sends the plan back to the session that wrote it, and the
    // words are what it picks its work back up with.
    terminal.keys("\x1b[B\r");
    terminal.wait_for(
        "what should change?",
        "the correction never asked for words",
    );
    terminal.keys("\r");
    terminal.wait_for(
        "this option needs an answer",
        "an empty correction was taken as an answer",
    );
    let drawn = terminal.drawn();
    assert!(
        !drawn.contains("enter records it"),
        "an option that needs words is never settled with a key:\n{drawn}"
    );
}

#[test]
fn resolve_gate_without_an_option_shows_the_documents_its_gate_shows() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let (repo, home) = (&checkout.repo, &checkout.home);
    // Off a terminal the gate parks the run, and it is answered later.
    let parked = yunta_in!(repo, home, &RUN);
    assert_eq!(parked.status.code(), Some(3), "{}", stderr(&parked));
    let run_id = run_id_from(&parked);

    let terminal = yunta_on_terminal!(repo, home, &["resolve-gate", &run_id]);
    terminal.wait_for(
        "2  adjust",
        "the run's own menu was never put to the terminal",
    );
    let drawn = terminal.drawn();
    assert!(
        drawn.contains("plan of `plan`")
            && drawn.contains("T001 — Make it")
            && drawn.contains("done when    the file exists"),
        "resolve-gate offered a decision without the plan it decides on:\n{drawn}"
    );
}

#[test]
fn status_node_prints_a_plan_whole() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let (repo, home) = (&checkout.repo, &checkout.home);
    let parked = yunta_in!(repo, home, &RUN);
    let run_id = run_id_from(&parked);

    let shown = yunta_in!(repo, home, &["status", &run_id, "--node", "plan"]);
    assert!(shown.status.success(), "{}", stderr(&shown));
    let page = yunta_testkit::stdout(&shown);
    assert!(
        page.contains("plan of `plan`")
            && page.contains("T001 — Make it")
            && page.contains("$ test -f made.txt"),
        "the node's page reads the plan it wrote whole:\n{page}"
    );
}

#[test]
fn the_question_and_its_options_are_the_last_lines_before_the_menu() {
    let root = tempfile::tempdir().unwrap();
    let terminal = reviewing(root.path());
    terminal.wait_for("3  abort", "the gate never put its options on the console");
    let drawn = terminal.drawn();
    let plan = drawn.find("plan of `plan`").expect("the plan was shown");
    let question = drawn
        .find("decision: ◆ needs you")
        .expect("the decision was put");
    assert!(
        plan < question,
        "the plan is read first and the question asked last, beside the menu:\n{drawn}"
    );
    assert!(
        !drawn[question..].contains("T001 — Make it"),
        "nothing of the plan sits between the question and its options:\n{drawn}"
    );
}

/// A brief, the plan written from it, and a gate that shows both.
const BRIEFED_PLAN: &str = r#"
name: briefed-plan
nodes:
  - id: brief
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Write the brief."
    artifacts:
      produces: [brief.md]
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    depends_on: [brief]
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    message: "Plan registered. Approve?"
    shows:
      - { node: brief, name: brief.md }
      - { node: plan, kind: tasks }
"#;

/// The brief, then the plan.
const BRIEFED: &str = r##"
capabilities: { run_tools: true }
sessions:
  - match_prompt_contains: "Write the brief"
    effects:
      - { path: "{{run.staging}}/brief/brief.md", content: "# Global pack installation\n\nA pack installs for the user, not one project.\n" }
    outcome: { type: completed, summary: "briefed" }
  - match_prompt_contains: "Hand over the tasks"
    steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Make it"
            description: "Writes the file the run is about."
            tasks:
              - { id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], changes: [{ at: made.txt, what: "the file", code: "the file's first line" }], outcome: "made.txt exists", criteria: [{ cmd: "test -f made.txt", proves: "the file exists" }] }
    outcome: { type: completed, summary: "planned" }
"##;

#[test]
fn what_was_asked_is_read_before_the_plan_that_answers_it() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .config(
            "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
             - { adapter: mock, model: mock-model }\n",
        )
        .workflow("wf", BRIEFED_PLAN)
        .file("fixture.yaml", BRIEFED)
        .committed();
    let terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN);
    terminal.wait_for("2  abort", "the gate never put its options on the console");
    let drawn = terminal.drawn();

    let asked = drawn
        .find("Global pack installation — brief.md of `brief`")
        .expect(&drawn);
    let plan = drawn.find("plan of `plan`").expect(&drawn);
    assert!(asked < plan, "the brief is read before the plan:\n{drawn}");
    assert!(
        drawn[asked..plan].contains("A pack installs for the user, not one project."),
        "{drawn}"
    );
}
