//! The small runs whose work the design's run does not show.

use std::collections::BTreeMap;

use yunta_core::events::{EventPayload, SessionEvent, StoredEvent, TaskStatus};
use yunta_replay::frame::{NodeStanding, RunPhase};
use yunta_replay::RunState;
use yunta_testkit_ui::small_runs::work;

use crate::{built_twice, stand};

/// Whether the log up to `events`' end stands with both tasks at work.
fn both_at_work(events: &[StoredEvent]) -> bool {
    let state = RunState::replay(events);
    ["task-1", "task-2"]
        .iter()
        .all(|task| state.tasks.status(*task) == Some(TaskStatus::Running))
}

/// The task each session of a task that `events` opens works, by the session's id.
fn tasks_of_sessions(events: &[StoredEvent]) -> BTreeMap<String, String> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Session(SessionEvent::Opened(opened))) => Some((
                opened.session_id.as_str().to_string(),
                opened.task_id.as_ref()?.as_str().to_string(),
            )),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn two_tasks_work_at_once_and_each_call_names_its_tasks_session() {
    let run = built_twice(work::two_tasks).await;
    let events = run.events(run.run());
    let together = (1..=events.len()).find(|end| both_at_work(&events[..*end]));
    assert!(together.is_some(), "the two tasks never work at one seq");

    let sessions = tasks_of_sessions(&events);
    let mut written = BTreeMap::new();
    for event in &events {
        let Some(EventPayload::Session(SessionEvent::Message(call))) = event.payload() else {
            continue;
        };
        let (Some(target), Some(session)) = (&call.target, &call.session) else {
            continue;
        };
        if let Some(task) = sessions.get(session.as_str()) {
            written.insert(format!("{target:?}"), task.clone());
        }
    }
    let task_of = |file: &str| {
        written
            .iter()
            .find(|(target, _)| target.contains(file))
            .map(|(_, task)| task.as_str())
    };
    assert_eq!(task_of("greeting.txt"), Some("task-1"), "{written:?}");
    assert_eq!(task_of("farewell.txt"), Some("task-2"), "{written:?}");
    assert!(matches!(stand(&run).await.frame.phase, RunPhase::Finished));
}

#[tokio::test]
async fn a_node_its_mode_leaves_out_stands_skipped() {
    let run = built_twice(work::mode_skips).await;
    let stand = stand(&run).await;
    assert!(
        matches!(stand.frame.phase, RunPhase::Finished),
        "{:?}",
        stand.frame.phase
    );
    let extra = stand
        .frame
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "extra")
        .expect("the node the mode leaves out");
    assert_eq!(extra.state, NodeStanding::Skipped);
}
