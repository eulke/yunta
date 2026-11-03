//! A plan put in front of the person approving it, driven end to end on a
//! terminal: the gate says what the plan is, a correction without words is
//! asked again, and one with words sends the plan back to the planner before
//! the gate asks once more.

use yunta_testkit::{run_id_from, stderr, stdout, yunta_in, yunta_on_terminal, Checkout, Terminal};

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
dialect: claude-code
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
    checkout_with(root, REVIEWED_PLAN)
}

/// A checkout whose workflow is `workflow`, planned by the `PLANNER` fixture.
fn checkout_with(root: &std::path::Path, workflow: &str) -> Checkout {
    Checkout::under(root)
        .config(
            "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
             - { adapter: mock, model: mock-model }\n",
        )
        .workflow("wf", workflow)
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
    terminal.wait_on_screen("▸ ", "the gate never put its card on the console");
    let screen = terminal.screen();
    assert!(
        screen.holds("Make it") && screen.holds("1 task"),
        "the card says what the plan it asks about is: {screen:#?}"
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
    terminal.wait_on_screen(
        "2 tasks",
        "the gate never said what the plan the correction made is",
    );
    // `approve`, the first option, with nothing to add.
    terminal.wait_on_screen("▸ approve", "the gate never asked about the second plan");
    terminal.keys("\r");

    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
}

#[test]
fn an_option_that_asks_requires_its_words() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = reviewing(root.path());
    terminal.wait_on_screen("▸ ", "the gate never put its card on the console");
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
fn resolve_gate_without_an_option_offers_the_plan_its_gate_shows() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let (repo, home) = (&checkout.repo, &checkout.home);
    // Off a terminal the gate parks the run, and it is answered later.
    let parked = yunta_in!(repo, home, &RUN);
    assert_eq!(parked.status.code(), Some(3), "{}", stderr(&parked));
    let run_id = run_id_from(&parked);

    let mut terminal = yunta_on_terminal!(repo, home, &["resolve-gate", &run_id]);
    terminal.wait_on_screen("▸ ", "the run's own card was never put to the terminal");
    let screen = terminal.screen();
    assert!(
        screen.holds("r reads the plan") && screen.holds("Make it"),
        "resolve-gate offered a decision without the plan it decides on: {screen:#?}"
    );
    terminal.keys("r");
    terminal.wait_on_screen("Writes the file the run is about.", "the plan never opened");
    terminal.keys("j");
    terminal.wait_on_screen("the file's first line", "its task never came up");
}

#[test]
fn resolve_gate_lists_the_words_an_option_asks_for() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let run = yunta_in!(&checkout.repo, &checkout.home, &RUN);
    let run_id = run_id_from(&run);
    let listed = yunta_in!(&checkout.repo, &checkout.home, &["resolve-gate", &run_id]);
    assert!(!listed.status.success());
    let said = stderr(&listed);
    let adjust = said
        .lines()
        .find(|line| line.trim_end().contains(" adjust"))
        .expect("the menu lists adjust");
    assert!(
        adjust.contains("--text"),
        "the option asks for words, so its command carries them:\n{said}"
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
        page.contains("plan: 1 task")
            && page.contains("T001: Make it")
            && page.contains("test -f made.txt"),
        "the node's page reads the plan it wrote whole:\n{page}"
    );
}

/// The id of the one run under `home`, which is the run this test started.
/// The engine created its directory, so the id is read from there, not from
/// the console that a check then compares with it.
fn the_only_run_under(home: &std::path::Path) -> String {
    let mut runs = std::fs::read_dir(yunta_testkit::runs_root(home))
        .expect("the run's directory exists under the home")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned());
    let id = runs.next().expect("a run was started under the home");
    assert!(runs.next().is_none(), "more than one run is under the home");
    id
}

/// `--quiet` says the run's id alone on the first line, and that id is the one
/// run under `home`. A plan with nothing weighing on it leaves nothing above its
/// card, so the question comes next, with the run's handle at its far edge.
#[test]
fn the_card_opens_right_under_the_run_with_the_question_and_its_options() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN);
    terminal.wait_on_screen("▸ ", "the gate never put its card on the console");
    let drawn = terminal.drawn();
    let id = the_only_run_under(&checkout.home);
    let lines: Vec<&str> = drawn
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert!(
        lines.first().is_some_and(|line| line.trim() == id),
        "the first line is not the id of the run that was started:\n{drawn}"
    );
    assert!(
        !drawn.contains("decision · needs you") && !drawn.contains("T001: Make it"),
        "nothing of the plan is above the card:\n{drawn}"
    );
    assert!(
        drawn.contains(yunta_testkit::handle(&id)),
        "the card's far edge does not name the run:\n{drawn}"
    );
    let question = drawn
        .find("◆ Plan registered. Approve?")
        .expect("the decision was put");
    let asked = &drawn[question..];
    assert!(
        asked.contains("Pause here: nothing runs"),
        "the decision offers to pause the run among its options:\n{drawn}"
    );
    assert!(
        asked.lines().any(|line| {
            // A decision's lines open with its edge.
            line.trim_start()
                .trim_start_matches(['|', '▎'])
                .trim_start()
                .starts_with("▸ approve")
        }),
        "the decision draws its options as rows, approve among them:\n{drawn}"
    );
    assert!(
        !drawn.contains("  1  approve:"),
        "the options are rows of the decision, not a numbered list:\n{drawn}"
    );
}

/// `REVIEWED_PLAN` with its gate's `message:` taken out, so the gate asks to
/// approve the change its plan's summary names.
fn without_message() -> String {
    REVIEWED_PLAN.replace("    message: \"Plan registered. Approve?\"\n", "")
}

/// A gate with no `message:` asks to approve the change its plan's summary
/// names, on a terminal and in the status of a run parked on it.
#[test]
fn a_gate_with_no_message_asks_to_approve_its_plan() {
    let workflow = without_message();
    assert!(
        !workflow.contains("message:"),
        "the gate still has a message:\n{workflow}"
    );

    let on_terminal = tempfile::tempdir().unwrap();
    let checkout = checkout_with(on_terminal.path(), &workflow);
    let terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN);
    terminal.wait_on_screen("▸ ", "the gate never put its card on the console");
    let drawn = terminal.drawn();
    assert!(
        drawn.contains("◆ Approve the plan: Make it?"),
        "the decision asks to approve the change the plan makes:\n{drawn}"
    );

    let parked_in = tempfile::tempdir().unwrap();
    let checkout = checkout_with(parked_in.path(), &workflow);
    let parked = yunta_in!(&checkout.repo, &checkout.home, &RUN);
    assert_eq!(parked.status.code(), Some(3), "{}", stderr(&parked));
    let run_id = run_id_from(&parked);
    let status = yunta_in!(&checkout.repo, &checkout.home, &["status", &run_id]);
    assert!(status.status.success(), "{}", stderr(&status));
    let said = stdout(&status);
    assert!(
        said.contains("Approve the plan: Make it?"),
        "the status of the parked run says what it waits on:\n{said}"
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
dialect: claude-code
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
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN);
    terminal.wait_on_screen("▸ ", "the gate never put its card on the console");
    terminal.keys("r");
    terminal.wait_on_screen(
        "A pack installs for the user, not one project.",
        "the brief is not what the plan's desk opens on",
    );
    terminal.keys("j");
    terminal.wait_on_screen(
        "Writes the file the run is about.",
        "what the plan says of itself does not follow it",
    );
}
