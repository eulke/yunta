//! What a person meets when a run stops and asks them something, driven
//! end to end: a real `yunta run` with a terminal on both ends of its
//! stdio, so the console surface engages instead of degrading to a
//! pause, and the test types the keys a person would.
//!
//! A terminal is the only place these behaviors exist. A pasted line
//! break is the byte a typed Enter is; an arrow is an escape sequence
//! and not a character; Escape is a decline and Ctrl-C is not; and the
//! read that waits for all of them has to stay off the run's own
//! runtime, where the Ctrl-C handler and the per-node run-tools
//! listener are waiting too.

use std::path::Path;

use yunta_core::events::{EventPayload, GateEvent, GateResolvedPayload, HumanChoice};
use yunta_testkit::{runs_root, wait_until, yunta_on_terminal, Checkout, Terminal};

#[path = "console_interaction/card.rs"]
mod card;

#[path = "console_interaction/round.rs"]
mod round;

#[path = "console_interaction/handing_back.rs"]
mod handing_back;

#[path = "console_interaction/init.rs"]
mod init;

#[path = "console_interaction/questions.rs"]
mod questions;

/// What a decision put to a person points at first, by the option it offers first.
const POINTED: &str = "▸ ";

/// A node that asks: the session hands the questions over and ends, and
/// the round with the person happens after it closes.
const ASKING: &str = "\
name: asking
nodes:
  - id: ask
    kind: prompt
    runner: executor
    prompt: \"Ask what has to be known before going on.\"
    artifacts:
      produces: [questions]
";

/// The session that hands the questions over, scripted — the round a
/// person answers is reached with no agent installed.
const HANDED_THEM_OVER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
QUESTIONS
    outcome: { type: completed, summary: asked }
"#;

const CONFIG: &str = "\
defaults:
  isolation: none
runners:
  executor:
    - { adapter: mock, model: mock-model }
";

/// The state root a run started under `root` keeps its runs in — the
/// same directory [`asking`] and [`gated`] hand the run.
fn home(root: &Path) -> std::path::PathBuf {
    root.join("state")
}

/// The answers the run under `root` recorded for its `ask` node, or
/// `None` while it has recorded none. The run's artifact view keeps a
/// producer's documents under its node, and the answers to a node's
/// questions are a document of that same node.
fn answers(root: &Path) -> Option<String> {
    let run = run_dir(root)?;
    std::fs::read_to_string(run.join("artifacts/ask/answers.yaml")).ok()
}

/// The choice the run started under `root` recorded at its gate, or
/// `None` while it has recorded none.
fn recorded(root: &Path) -> Option<HumanChoice> {
    let run_id = run_dir(root)?.file_name()?.to_str()?.parse().ok()?;
    let storage = yunta_storage::Storage::open(&home(root).join("yunta.db")).ok()?;
    let events = storage.events_for_run(&run_id).ok()?;
    events.iter().find_map(|event| match event.payload() {
        Some(EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Chosen(choice)))) => {
            Some(choice.clone())
        }
        _ => None,
    })
}

/// The directory the run started under `root` keeps its own state in,
/// or `None` before there is a run.
fn run_dir(root: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(runs_root(&home(root)))
        .ok()?
        .flatten()
        .next()
        .map(|run| run.path())
}

/// `questions` nested under the fixture's key, every line twelve spaces in.
fn nested(questions: &str) -> String {
    questions
        .lines()
        .map(|line| format!("{}{line}", " ".repeat(12)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A run of [`ASKING`] on a terminal, its node having produced
/// `questions`.
fn asking(root: &Path, questions: &str) -> Terminal {
    let checkout = Checkout::under(root)
        .config(CONFIG)
        .workflow("wf", ASKING)
        .file(
            "fixture.yaml",
            &HANDED_THEM_OVER.replace("QUESTIONS", &nested(questions)),
        )
        .committed();
    let (repo, home) = (checkout.repo, checkout.home);
    // `--quiet` leaves the run's progress out: what a person is asked is
    // not progress, and the round is what these tests read.
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

/// A run that reaches a gate with its reroutes already spent — an
/// escalation put to a person with no agent anywhere in it.
fn gated(root: &Path) -> Terminal {
    let checkout = Checkout::under(root)
        .working_in_place()
        .workflow(
            "wf",
            "name: gated\nnodes:\n  - id: lint\n    kind: bash\n    run: \"false\"\n    \
             on_failure: { goto: fix, max_reroutes: 0 }\n  - id: fix\n    kind: bash\n    \
             run: \"true\"\n",
        )
        .committed();
    let (repo, home) = (checkout.repo, checkout.home);
    yunta_on_terminal!(&repo, &home, &["run", "wf.yaml"])
}

/// A run that stops on an unresolved internal gate: the question its
/// author wrote is the escalation's summary, and who the gate is
/// assigned to — which that question never says — is the evidence the
/// engine attaches under it.
fn approving(root: &Path) -> Terminal {
    let checkout = Checkout::under(root)
        .working_in_place()
        .workflow(
            "wf",
            "name: approving\nnodes:\n  - id: ship\n    kind: gate\n    assignee: the release \
             lead\n    message: \"Ship what is on the branch?\"\n",
        )
        .committed();
    let (repo, home) = (checkout.repo, checkout.home);
    yunta_on_terminal!(&repo, &home, &["run", "wf.yaml"])
}
