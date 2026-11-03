//! What a run on a terminal does besides its words: a frame of its spin for each
//! event, each drawing one update, and the window told once when the run needs its
//! person. With the real binary, on a pty.

use yunta_testkit::{yunta_on_terminal, Checkout};

const WORK: &str = r#"
name: work
nodes:
  - id: implement
    kind: prompt
    runner: planner
    prompt: "Do it."
"#;

const SESSION: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: tool_use, name: Bash, target: "cargo build", after_ms: 400 }
      - { type: tool_use, name: Bash, target: "cargo test", after_ms: 400 }
      - { type: tool_use, name: Bash, target: "git status", after_ms: 400 }
    outcome: { type: completed, summary: "done" }
"#;

const ASKS: &str = r#"
name: asks
nodes:
  - id: ship
    kind: gate
    assignee: lead
    message: "Ship it?"
    options: [approve]
"#;

const RUNNER: &str = "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
                      - { adapter: mock, model: mock-model }\n";

const RUN_WORK: [&str; 6] = [
    "run",
    "wf.yaml",
    "--adapter",
    "mock",
    "--fixture",
    "fixture.yaml",
];

/// The frames a running node's spin turns through.
const SPIN: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// How many of the spin's frames `drawn` holds.
fn frames_in(drawn: &str) -> usize {
    SPIN.iter().filter(|frame| drawn.contains(**frame)).count()
}

/// A checkout that runs a session of three calls.
fn working(root: &std::path::Path) -> Checkout {
    Checkout::under(root)
        .config(RUNNER)
        .workflow("wf", WORK)
        .file("fixture.yaml", SESSION)
        .committed()
}

/// A checkout whose run stops to ask its person, under `config`.
fn asking(root: &std::path::Path, config: &str) -> Checkout {
    Checkout::under(root)
        .config(config)
        .workflow("wf", ASKS)
        .committed()
}

/// A gate answered with its first option, with nothing to add.
fn answered(terminal: &mut yunta_testkit::Terminal) -> String {
    terminal.wait_for("abort", "the gate never put its options on the console");
    terminal.keys("\r");
    terminal.wait_for(
        "enter records it, n adds a note",
        "approving asked for words it does not need",
    );
    terminal.keys("\r");
    terminal.ended()
}

#[test]
fn a_running_node_turns_as_its_events_arrive() {
    let root = tempfile::tempdir().unwrap();
    let checkout = working(root.path());
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN_WORK);
    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    assert!(drawn.contains("implement running for"), "{drawn}");
    assert!(frames_in(&drawn) >= 2, "it never turned:\n{drawn}");
}

#[test]
fn a_reader_who_turned_motion_off_gets_the_same_words_and_no_spin() {
    let root = tempfile::tempdir().unwrap();
    let checkout = working(root.path());
    let vars = [("YUNTA_MOTION", "off")];
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN_WORK, &vars);
    let drawn = terminal.ended();
    assert!(terminal.ran_to_the_end(), "{drawn}");
    assert!(drawn.contains("● implement running for"), "{drawn}");
    assert_eq!(frames_in(&drawn), 0, "{drawn}");
}

#[test]
fn each_drawing_of_the_region_is_one_update() {
    let root = tempfile::tempdir().unwrap();
    let checkout = working(root.path());
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN_WORK);
    let drawn = terminal.ended();
    let marks: Vec<char> = drawn
        .split("\u{1b}[?2026")
        .skip(1)
        .filter_map(|rest| rest.chars().next())
        .collect();
    assert!(marks.len() > 2, "{drawn}");
    assert!(
        marks.chunks(2).all(|pair| pair == ['h', 'l']),
        "an update opens, closes, and only then opens again: {marks:?}"
    );
}

#[test]
fn the_window_is_told_once_when_the_run_needs_its_person_and_given_back_after() {
    let root = tempfile::tempdir().unwrap();
    let checkout = asking(root.path(), "defaults:\n  isolation: none\n");
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &["run", "wf.yaml"]);
    terminal.wait_for("abort", "the gate never put its options on the console");
    let asked = terminal.drawn();
    let told = asked
        .split("\u{1b}]2;")
        .nth(1)
        .and_then(|rest| rest.split("\u{1b}\\").next())
        .unwrap_or_else(|| panic!("the window was never told:\n{asked}"));
    assert!(
        told.starts_with('◆') && told.contains("needs you"),
        "{told}"
    );
    assert!(told.ends_with("asks (default)"), "{told}");

    let drawn = answered(&mut terminal);
    assert!(terminal.ran_to_the_end(), "{drawn}");
    assert_eq!(drawn.matches("\u{1b}]2;").count(), 1, "{drawn}");
    assert_eq!(drawn.matches("\u{1b}[22;2t").count(), 1, "{drawn}");
    assert_eq!(drawn.matches("\u{1b}[23;2t").count(), 1, "{drawn}");
    assert_eq!(
        drawn.matches('\u{7}').count(),
        0,
        "no bell unless it is asked for: {drawn}"
    );
}

#[test]
fn a_person_who_asked_for_a_bell_gets_one() {
    let root = tempfile::tempdir().unwrap();
    let config = "defaults:\n  isolation: none\nterminal:\n  bell: true\n";
    let checkout = asking(root.path(), config);
    let mut terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &["run", "wf.yaml"]);
    let drawn = answered(&mut terminal);
    assert!(terminal.ran_to_the_end(), "{drawn}");
    assert_eq!(drawn.matches('\u{7}').count(), 1, "{drawn}");
}
