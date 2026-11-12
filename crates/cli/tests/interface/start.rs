//! A run started end to end: what is asked for typed at the first run of a repository
//! set up with no run, the workflow, its inputs and its adapter chosen, and the run
//! started detached by this build and followed to its end; and an input left empty,
//! refused as `yunta run` refuses it.

use super::*;

use surco::components::Note;
use surco::doc::Block;
use yunta_testkit::workflows::GREETS;
use yunta_testkit::{runs_root, stderr, wait_until, yunta_at, Checkout};

/// A workflow that takes what was asked for, and whom to greet, which nothing defaults.
const NAMES: &str = "name: named\ninputs:\n  ask: { type: string, request: true }\n  \
                     who: { type: string, description: \"whom it greets\" }\nnodes:\n  \
                     - { id: say, kind: bash, run: \"true\" }\n";

/// Each character of `words` typed, then Enter.
fn typed(words: &str) -> Vec<Stroke> {
    words
        .chars()
        .map(Stroke::Insert)
        .chain([Stroke::Enter])
        .collect()
}

/// The runs under `home`, by their whole ids.
fn runs_under(home: &Path) -> Vec<String> {
    std::fs::read_dir(runs_root(home))
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// What is asked for at the first run, with the workflow and the adapter each list
/// opens on, starts the run detached, the request in the input the workflow reads it
/// from; the application follows it to its end.
#[tokio::test]
async fn a_run_started_at_the_first_run_is_followed_to_its_end() {
    let checkout = Checkout::new()
        .working_in_place()
        .file(".yunta/workflows/greet.yaml", GREETS)
        .committed();
    let performer = Arc::new(Performer::new(workspace(&checkout.repo, &checkout.home)));
    let (opened, asked) = State::opened(now());

    let mut strokes = typed("greet ana");
    strokes.extend([Stroke::Enter, Stroke::Enter]);
    let (state, _) = drive(opened, asked, &performer, &strokes).await;

    let [id] = runs_under(&checkout.home)
        .try_into()
        .expect("one run started");
    let screen = state.screen(&Markdown.look());
    assert_eq!(
        screen.trail.last().map(String::as_str),
        Some(yunta_testkit::handle(&id)),
        "the run is followed in its stage"
    );
    let dir = runs_root(&checkout.home).join(&id);
    let log = || std::fs::read_to_string(dir.join("events.jsonl")).unwrap_or_default();
    wait_until(
        || log().contains("\"run_finished\""),
        || format!("the run never finished:\n{}", log()),
    );
    let manifest = std::fs::read_to_string(dir.join("manifest.yaml")).unwrap();
    assert!(manifest.contains("greet ana"), "{manifest}");
}

/// An input left empty is refused by admission as `yunta run` refuses it, said where
/// the person stands, and no run is made.
#[tokio::test]
async fn an_input_left_empty_is_refused_as_yunta_run_refuses_it() {
    let checkout = Checkout::new()
        .working_in_place()
        .file(".yunta/workflows/named.yaml", NAMES)
        .committed();
    let refused_at_the_command_line =
        yunta_at!(&checkout, &["run", "named", "--input", "ask=greet ana"]);
    assert!(!refused_at_the_command_line.status.success());
    let performer = Arc::new(Performer::new(workspace(&checkout.repo, &checkout.home)));
    let (opened, asked) = State::opened(now());

    let mut strokes = typed("greet ana");
    strokes.extend([Stroke::Enter, Stroke::Enter, Stroke::Enter]);
    let (state, _) = drive(opened, asked, &performer, &strokes).await;

    let screen = state.screen(&Markdown.look());
    let said: Vec<String> = screen
        .main
        .doc
        .blocks()
        .iter()
        .filter_map(|block| match block {
            Block::Note(Note(said)) => Some(said.clone()),
            _ => None,
        })
        .collect();
    let refusal = said.last().expect("the refusal is said");
    let printed = stderr(&refused_at_the_command_line);
    for line in refusal.lines() {
        assert!(printed.contains(line), "`{line}` is not in:\n{printed}");
    }
    assert!(runs_under(&checkout.home).is_empty(), "no run is made");
}
