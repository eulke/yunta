use std::path::{Path, PathBuf};

use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::events::StoredEvent;
use yunta_core::{NodeId, NodeKind, Pid, RunId, TaskId, Workflow};
use yunta_replay::frame::{Moment, RunFrame};
use yunta_replay::registry::Engine;
use yunta_replay::RunState;

use super::Node;
use crate::page::Page;

/// The design's run, as it is kept beside the application's tests.
fn designed() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../testkit-ui/fixtures/designed")
}

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/node")
}

/// The title the design's plan gives its one task.
const TITLE: &str = "Add explicit global pack install target";

/// Whether a process drives the run where it is cut.
#[derive(Clone, Copy)]
enum Driven {
    /// An engine drives it: it is at work.
    Alive,
    /// None does: it is parked, waiting on a person.
    Parked,
}

/// The design run's log, cut at `seq`.
fn logged_to(seq: u64) -> Vec<StoredEvent> {
    std::fs::read_to_string(designed().join("log.jsonl"))
        .expect("the design's run is kept")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a logged event"))
        .filter(|event: &StoredEvent| event.seq.get() <= seq)
        .collect()
}

/// The design run's workflow, as its manifest froze it.
fn workflow() -> Workflow {
    let text = std::fs::read_to_string(designed().join("workflow.yaml")).expect("its workflow");
    Workflow::parse(&text).expect("the design's workflow reads")
}

/// The page of the design run's node `node`, its log cut at `seq` and read at its last
/// event, with no document read: a page reads none of its own.
fn page(node: &str, seq: u64, driven: Driven) -> Node {
    let (events, workflow, id) = (logged_to(seq), workflow(), NodeId::from(node));
    let mut state = RunState::default();
    let moments: Vec<Moment> = events
        .iter()
        .filter_map(|event| state.chronicled(event))
        .filter(|moment| moment.node.as_ref() == Some(&id))
        .collect();
    let run = events[0].run_id.clone();
    let engine = match driven {
        Driven::Alive => Engine::this(Pid::current()),
        Driven::Parked => Engine::Unrecorded,
    };
    let now = state.clock().last_at().expect("a run with events");
    let frame = RunFrame::of(&run, &workflow, &state, None, &engine, now);
    let declares = workflow.iter_nodes().find(|declared| declared.id == id);
    let declares = declares.expect("the node is declared");
    let works = matches!(declares.kind, NodeKind::Loop { .. });
    let tasks: Vec<(TaskId, Option<String>)> = state
        .tasks
        .iter()
        .filter(|_| works)
        .map(|(task, _)| (task.clone(), Some(TITLE.to_string())))
        .collect();
    let artifacts = declares.artifacts.as_ref();
    Node {
        run_id: RunId::from(run.as_str()),
        node: frame
            .nodes
            .into_iter()
            .find(|frame| frame.id == id)
            .expect("its frame"),
        run_dir: PathBuf::from("/home/me/.yunta/runs").join(run.as_str()),
        cwd: PathBuf::from("/home/me/project"),
        home: Some(PathBuf::from("/home/me")),
        documents: Vec::new(),
        unread: Vec::new(),
        declared: artifacts
            .map(|artifacts| artifacts.produces.clone())
            .unwrap_or_default(),
        tasks,
        state,
        moments,
    }
}

/// A node's page says in print what its pane shows, in each way a node stands: a session
/// at work, a loop with a task red and its scope, a node finished, one failed, and one
/// waiting on a person.
#[test]
fn a_nodes_page_matches_its_goldens_in_each_way_a_node_stands() {
    let cases = [
        ("working", page("plan", 42, Driven::Alive)),
        ("loop", page("implement", 99, Driven::Alive)),
        ("finished", page("brief", 42, Driven::Alive)),
        ("failed", page("lint", 234, Driven::Alive)),
        ("waiting", page("grill", 19, Driven::Parked)),
    ];
    for (name, page) in &cases {
        for environment in &ENVIRONMENTS {
            let look = Look::of(environment);
            assert_golden(
                &environment.golden(&goldens(), name),
                &Terminal::on(look).draw(&page.document(&look)),
            );
        }
    }
}

/// A feed longer than the page prints shows its newest rows, the newest last, and says how
/// many stand above them.
#[test]
fn a_feed_longer_than_the_page_prints_says_how_many_rows_stand_above() {
    let page = page("implement", 159, Driven::Alive);
    let look = Look::of(&ENVIRONMENTS[1]);
    let drawn = Terminal::on(look).draw(&page.document(&look));
    let lines: Vec<&str> = drawn.lines().collect();
    let heading = lines
        .iter()
        .position(|line| line.starts_with("what it is doing"))
        .expect("the feed's heading");
    let above = lines[heading + 1];
    assert!(
        above.starts_with("... ") && above.ends_with(" above"),
        "{drawn}"
    );
    let rows = lines[heading + 2..]
        .iter()
        .take_while(|line| !line.is_empty())
        .count();
    assert_eq!(rows, super::FEED_ROWS, "{drawn}");
}
