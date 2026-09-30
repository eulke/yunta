//! A plan put in front of the person approving it, driven end to end on a
//! terminal: the gate draws the tasks it asks about, a correction without
//! words is asked again, and one with words sends the plan back to the
//! planner before the gate asks once more.

use yunta_testkit::{yunta_on_terminal, Checkout, Terminal};

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
              - { id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], changes: [{ at: made.txt, what: "the file" }], outcome: "made.txt exists", criteria: [{ cmd: "test -f made.txt", proves: "the file exists" }] }
    outcome: { type: completed, summary: "planned" }
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Make it"
            description: "Writes the file the run is about."
            tasks:
              - { id: T001, title: "Make it", description: "Writes made.txt.", scope: [made.txt], changes: [{ at: made.txt, what: "the file" }], outcome: "made.txt exists", criteria: [{ cmd: "test -f made.txt", proves: "the file exists" }] }
              - { id: T002, title: "Say it", description: "Writes said.txt.", scope: [said.txt], changes: [{ at: said.txt, what: "the file" }], outcome: "said.txt exists", criteria: [{ cmd: "test -f said.txt", proves: "it was said" }] }
    outcome: { type: completed, summary: "planned again" }
"#;

fn reviewing(root: &std::path::Path) -> Terminal {
    let checkout = Checkout::under(root)
        .config(
            "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
             - { adapter: mock, model: mock-model }\n",
        )
        .workflow("wf", REVIEWED_PLAN)
        .file("fixture.yaml", PLANNER)
        .committed();
    let (repo, home) = (checkout.repo, checkout.home);
    yunta_on_terminal!(
        &repo,
        &home,
        &[
            "run",
            "wf.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml",
            "--quiet",
        ]
    )
}

#[test]
fn a_plan_is_reviewed_at_its_gate_and_a_correction_sends_it_back_with_the_words() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = reviewing(root.path());
    terminal.wait_for("3  abort", "the gate never put its options on the console");
    let drawn = terminal.drawn();
    assert!(
        drawn.contains("what you are deciding on")
            && drawn.contains("Writes the file the run is about.")
            && drawn.contains("T001 — Make it")
            && drawn.contains("done when     the file exists")
            && !drawn.contains("test -f made.txt")
            && drawn.contains("/artifacts/plan/tasks.md"),
        "the gate shows the plan it asks about, explained:\n{drawn}"
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
    terminal.wait_for("anything to add?", "approving asked nothing further");
    terminal.keys("\r");

    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
}
