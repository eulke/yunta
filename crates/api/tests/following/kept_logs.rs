//! The follower beside `yunta status` on every log an earlier binary wrote that the
//! replay tests keep, and on one of them broken in its middle: each loaded into a store
//! of its own with a manifest naming its nodes, read whole from its middle, read on in
//! batches as the rest is appended, then read whole again. Every reader folds a log by
//! replay's one fold, so each stands where the log's replay stands.

use std::path::Path;

use yunta_api::FollowedRun;
use yunta_core::events::{
    EventBody, EventDraft, EventPayload, NodeEvent, NodeFinishedPayload, NodeOutcome, StoredEvent,
    TokenUsage,
};
use yunta_core::NodeId;
use yunta_replay::RunState;

use super::{next_of, whole, World};

/// How many batches the part of a log past its middle is appended in, each read before
/// the next is appended, so a batch is folded onto a run already read on.
const BATCHES: usize = 3;

/// Each log the replay tests keep, by its name, as it was recorded.
fn kept_logs() -> Vec<(String, Vec<StoredEvent>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../replay/tests/fixtures/logs");
    let mut logs: Vec<(String, Vec<StoredEvent>)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).unwrap();
            let events = text
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            (name, events)
        })
        .collect();
    logs.sort_by(|one, other| one.0.cmp(&other.0));
    logs
}

/// `log` broken right past its middle: a node's terminal that names no node, which no
/// log can take, so it reaches a follower after the run was read whole, with events
/// still to come after it.
fn broken(log: &[StoredEvent]) -> Vec<StoredEvent> {
    let middle = log.len() / 2;
    let finished =
        NodeFinishedPayload::new(NodeOutcome::Summary("ok".into()), TokenUsage::default());
    let refused = StoredEvent {
        node_id: None,
        body: EventBody::Known(EventPayload::Node(NodeEvent::Finished(finished))),
        ..log[middle].clone()
    };
    let (before, after) = log.split_at(middle + 1);
    before
        .iter()
        .chain(std::iter::once(&refused))
        .chain(after)
        .cloned()
        .collect()
}

/// A workflow with a bash node for each node `log` names, in the order it first names
/// them.
fn workflow(log: &[StoredEvent]) -> String {
    let mut named: Vec<&NodeId> = Vec::new();
    for node in log.iter().filter_map(|event| event.node_id.as_ref()) {
        if !named.contains(&node) {
            named.push(node);
        }
    }
    let mut yaml = String::from("name: kept\nnodes:\n");
    for node in named {
        yaml.push_str(&format!(
            "  - {{ id: {node}, kind: bash, run: \"true\" }}\n"
        ));
    }
    yaml
}

/// `events` appended to the store of `world`, each under its node, at its instant.
fn appended(world: &World, events: &[StoredEvent]) {
    for event in events {
        let payload = event
            .payload()
            .expect("a kept log's kinds are known")
            .clone();
        let draft = EventDraft::new(event.run_id.clone(), event.node_id.clone(), payload).unwrap();
        world
            .bench
            .storage
            .append_at(&draft, event.timestamp)
            .unwrap();
    }
}

/// `log` loaded into the store of `world` as its run writes it: the run followed once
/// the first half stands, then read on as the rest is appended batch by batch. Hands back
/// the run as the following's news fold it, once the log's last event is handed.
async fn read_on_from_its_middle(world: &World, log: &[StoredEvent]) -> FollowedRun {
    let run = log[0].run_id.clone();
    world.dir_of(&run, &workflow(log)).await;
    let (first, rest) = log.split_at(log.len() / 2);
    appended(world, first);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;
    for batch in rest.chunks(rest.len().div_ceil(BATCHES)) {
        appended(world, batch);
        let head = world.logged(&run).last().map(|event| event.seq);
        while kept.state().head() != head {
            next_of(&mut following, &mut kept).await;
        }
    }
    kept
}

/// The log `name` holds, loaded into a store of its own, read on from its middle and then
/// read whole again. The run the following's news are folded into, the run read whole
/// again, what the events it is read whole with fold to and the log's replay stand alike,
/// at one head; their frames and `yunta status`'s stand alike at one instant. Hands back
/// the log's replay.
async fn followed_as_replayed(name: &str, log: &[StoredEvent]) -> RunState {
    let world = World::new();
    let kept = read_on_from_its_middle(&world, log).await;
    let run = kept.run().clone();
    let logged = world.logged(&run);
    let replayed = RunState::replay(&logged);
    let (read, events) = whole(&mut world.follow(&run).await).await;
    assert_eq!(
        events, logged,
        "{name}: read whole with every event of its log"
    );
    let mut folded = RunState::default();
    events.iter().for_each(|event| folded.fold(event));
    for (reader, state) in [
        ("folded from the news", kept.state()),
        ("read whole again", read.state()),
        ("folded from the events read whole", &folded),
    ] {
        assert_eq!(
            state, &replayed,
            "{name}: the run {reader} stands as its log replays"
        );
        assert_eq!(
            state.head(),
            replayed.head(),
            "{name}: the run {reader} at its head"
        );
    }
    let status = world.project.run(&run).await.unwrap().frame();
    assert_eq!(
        world.frame(&kept),
        status,
        "{name}: framed as `yunta status` frames it"
    );
    assert_eq!(
        world.frame(&read),
        status,
        "{name}: read whole, framed as `yunta status`"
    );
    replayed
}

/// Every reader of a log folds it by replay's one fold: the follower, on each kept log
/// and on one broken in its middle, stands where `yunta status` and the log's replay
/// stand, and stops at the break where replay stops.
///
/// Reached by: `crates/api/src/follow/followed.rs`, `crates/api/src/follow/following.rs`,
/// `crates/api/src/run/reading.rs`
#[tokio::test]
async fn the_follower_stands_where_yunta_status_does_on_every_kept_log() {
    let logs = kept_logs();
    assert!(!logs.is_empty(), "the replay tests keep logs");
    for (name, log) in &logs {
        let replayed = followed_as_replayed(name, log).await;
        assert_eq!(replayed.broken, None, "{name} replays whole");
    }
    let (_, run) = logs
        .iter()
        .find(|(name, _)| name == "claude-run")
        .expect("the replay tests keep a run of claude-code");
    let replayed = followed_as_replayed("claude-run, broken", &broken(run)).await;
    assert!(replayed.broken.is_some(), "the log breaks in its middle");
}
