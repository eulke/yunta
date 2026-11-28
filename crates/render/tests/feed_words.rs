//! A node's feed says each moment in the words its chronicle logs: the row a pane shows and
//! the line `status --follow` prints for the same moment differ only in the time and the
//! subject the line opens with.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use yunta_core::events::StoredEvent;
use yunta_core::NodeId;
use yunta_render::chronicle::{feed, in_feed, logged};
use yunta_replay::frame::{chronicle, Happening, Moment};
use yunta_replay::RunState;

/// Where the workspace's crates stand.
fn crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The design's run, as it is kept.
fn designed() -> PathBuf {
    crates().join("testkit-ui/fixtures/designed/log.jsonl")
}

/// The logs a feed is read from, each by its path: the logs earlier binaries wrote, as replay
/// checks them in, and the design's run.
fn logs() -> Vec<PathBuf> {
    let checked_in = crates().join("replay/tests/fixtures/logs");
    let mut logs: Vec<PathBuf> = std::fs::read_dir(&checked_in)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|it| it == "jsonl"))
        .collect();
    logs.sort();
    logs.push(designed());
    logs
}

/// A terminal that draws Unicode, as wide as a line gets.
fn look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::of(Some(120), None),
        motion: Motion::Moves,
    }
}

/// The moments of `node` its feed lists, in the log's order.
fn listed(moments: &[Moment], node: &NodeId) -> Vec<Moment> {
    moments
        .iter()
        .filter(|it| it.node.as_ref() == Some(node) && in_feed(&it.happening))
        .cloned()
        .collect()
}

/// The events of the log at `path`, as recorded.
fn load(path: &Path) -> Vec<StoredEvent> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// The words of `text`, however many spaces part them.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

/// The words a logged line says after its time, in brackets, the mark it opens with where it
/// has one, and `node`, its subject, which a colon follows where the words remark on it.
fn after_time_and_subject(line: &str, node: &NodeId) -> Vec<String> {
    let mut said = words(line).into_iter().peekable();
    let time = said.next().unwrap_or_default();
    assert!(time.starts_with('[') && time.ends_with(']'), "{line}");
    if said
        .peek()
        .is_some_and(|mark| !mark.chars().any(char::is_alphanumeric))
    {
        said.next();
    }
    let subject = said.next().unwrap_or_default();
    assert!(
        subject == node.as_str() || subject == format!("{node}:"),
        "{line} opens with no subject `{node}`"
    );
    said.collect()
}

/// Every moment a node's feed lists says in its row the words its logged line says after its
/// time and its subject; a call of a loop whose tasks work at once names its task after its
/// verb, a word the line has not.
#[test]
fn every_row_of_a_feed_says_what_the_chronicle_logs_after_its_time_and_subject() {
    let look = look();
    let mut rows = 0;
    for path in logs() {
        let events = load(&path);
        let state = RunState::replay(&events);
        let moments = chronicle(&events);
        let nodes: BTreeSet<&NodeId> = moments.iter().filter_map(|it| it.node.as_ref()).collect();
        for node in nodes {
            let listed = listed(&moments, node);
            let tasks = state
                .nodes
                .get(node)
                .map(|record| record.session_tasks.clone())
                .unwrap_or_default();
            let fed = feed(&listed, &tasks, 0, 0, &look);
            assert_eq!(fed.rows.len(), listed.len(), "{}: {node}", path.display());
            for (row, moment) in fed.rows.iter().zip(&listed) {
                let told = after_time_and_subject(&logged(moment, &look).text(), node);
                let mut said = words(&row.line.text());
                let task = match &moment.happening {
                    Happening::Session(
                        yunta_core::events::session::happening::Happening::Called {
                            session: Some(session),
                            ..
                        },
                    ) => tasks.get(session),
                    _ => None,
                };
                if task.is_some_and(|task| said.get(1) == Some(&task.to_string())) {
                    said.remove(1);
                }
                assert_eq!(
                    said,
                    told,
                    "{}: {node}: {}",
                    path.display(),
                    row.line.text()
                );
                rows += 1;
            }
        }
    }
    assert!(rows > 0, "no log lists a moment in a feed");
}

/// A check that left a criterion red says so as the proof's pill says it, the word standing
/// alone between separators, and then the criterion: the design's run checks
/// `global-pack-install` red at `implement`.
#[test]
fn a_red_check_says_red_as_a_word_of_its_own() {
    let events = load(&designed());
    let node = NodeId::from("implement");
    let moments = listed(&chronicle(&events), &node);
    let state = RunState::replay(&events);
    let tasks = state
        .nodes
        .get(&node)
        .map(|record| record.session_tasks.clone())
        .unwrap_or_default();
    let rows: Vec<String> = feed(&moments, &tasks, 0, 0, &look())
        .rows
        .iter()
        .map(|row| row.line.text())
        .filter(|row| row.starts_with("global-pack-install post: 1 criterion"))
        .collect();
    assert!(!rows.is_empty(), "the design's run checks the task red");
    for row in rows {
        assert!(
            row.starts_with(
                "global-pack-install post: 1 criterion · red · `cargo test --workspace --test pack"
            ),
            "{row}"
        );
    }
}
