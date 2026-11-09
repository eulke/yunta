use std::path::Path;

use surco::words::Standing;
use yunta_core::events::{ChildEvent, EventPayload, RunEvent};
use yunta_core::events::{
    ChildRunCreatedPayload, NodeStartedPayload, RunFinishedPayload, RunMetrics, TokenUsage,
};
use yunta_core::{CommitSha, ConfigLayer, ContentHash, Isolation, NodeId, Pid};
use yunta_engine::lock::OwnerProbe;
use yunta_engine::EngineProcessFile;
use yunta_replay::registry::Prompt;
use yunta_testkit_core::{Captured, FixedClock, Log};

use super::*;
use crate::surface::{Delivery, EngineReading};
use yunta_core::events::NodeEvent;

const DRAWN: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P5";
const OTHER: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P6");

fn started() -> EventPayload {
    EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1)))
}

fn closed(terminal: TerminalState) -> EventPayload {
    EventPayload::Run(RunEvent::Finished(RunFinishedPayload {
        terminal_state: terminal,
        metrics: RunMetrics {
            cptv: None,
            tokens: TokenUsage::default(),
        },
        closed_by: None,
    }))
}

fn bore(child: &RunId) -> EventPayload {
    EventPayload::Children(ChildEvent::Created(ChildRunCreatedPayload {
        child_run_id: child.clone(),
        child_workflow_hash: ContentHash::sha256(b"child"),
    }))
}

#[test]
fn a_run_still_moving_has_no_successor() {
    let log = Log::for_run(DRAWN).node("build", started()).build();
    assert!(!succeeds(&RunState::replay(&log), &OTHER));
}

#[test]
fn a_run_that_closed_any_other_way_has_no_successor() {
    for terminal in [
        TerminalState::Done,
        TerminalState::Failed,
        TerminalState::Cancelled,
    ] {
        let log = Log::for_run(DRAWN)
            .node("build", started())
            .event(closed(terminal))
            .build();
        assert!(!succeeds(&RunState::replay(&log), &OTHER), "{terminal:?}");
    }
}

#[test]
fn the_run_appending_after_a_promoted_close_is_the_successor() {
    let log = Log::for_run(DRAWN)
        .node("build", started())
        .event(closed(TerminalState::Promoted))
        .build();
    assert!(succeeds(&RunState::replay(&log), &OTHER));
}

#[test]
fn a_child_this_run_bore_is_never_mistaken_for_its_successor() {
    let log = Log::for_run(DRAWN)
        .node("build", started())
        .event(bore(&OTHER))
        .event(closed(TerminalState::Promoted))
        .build();
    assert!(!succeeds(&RunState::replay(&log), &OTHER));
}

/// A run frozen with one node, `build`, which the frames below are about.
fn manifest() -> Manifest {
    Manifest {
        schema_version: 1,
        yunta_version: "0.0.0".to_string(),
        workflow: yunta_core::yaml::parse(
            "name: drawn\nnodes:\n  - { id: build, kind: bash, run: \"true\" }\n",
        )
        .expect("a workflow of one node parses"),
        config: ConfigLayer::default(),
        inputs: std::collections::BTreeMap::new(),
        prompts: std::collections::BTreeMap::new(),
        base_branch: "main".to_string(),
        base_commit: CommitSha::from_static("0123456789abcdef0123456789abcdef01234567"),
        isolation: Isolation::None,
        max_parallel_nodes: 1,
        workflow_hash: ContentHash::sha256(b"workflow"),
        config_hash: ContentHash::sha256(b"config"),
        paths: None,
        pack: None,
        project: None,
        adapter: None,
        frozen: None,
        provenance_gaps: Vec::new(),
        loaded_links: Vec::new(),
    }
}

/// The `run_created` a run's log opens with, which is what makes it a run
/// that moves rather than one not yet born.
fn born() -> EventPayload {
    EventPayload::Run(RunEvent::Created(yunta_core::events::RunCreatedPayload {
        checkout: None,
        manifest_hash: yunta_core::sha256_hex(b"manifest"),
        inputs: Default::default(),
        mode: Default::default(),
        promoted_from: None,
        yunta_schema: None,
        base_branch: "main".to_string(),
        base_commit: "0123456789abcdef0123456789abcdef01234567".into(),
        environment: None,
        left_out: Vec::new(),
        opens_on_base: false,
    }))
}

/// A painter on a run whose `build` node has started, reading the process
/// driving it the way `engine` says, and drawing lines nobody reads.
async fn painter_of(engine: EngineReading, at: &Path) -> Painter {
    let log = EventLog::open(&at.join("events.db"))
        .await
        .expect("a store opens under the test's own directory");
    let manifest = manifest();
    let run_id = RunId::from(DRAWN);
    let env = SurfaceEnv {
        run_id: &run_id,
        manifest: &manifest,
        prior: None,
        log: &log,
        clock: Arc::new(FixedClock),
        delivery: Delivery::Lines { reason: "a test" },
        glyphs: Glyphs::Ascii,
        engine,
    };
    let draw = Draw::Lines(crate::surface::lines::Lines::open(
        Box::new(Captured::default()),
        "a test",
    ));
    let seed = Log::for_run(DRAWN)
        .event(born())
        .node("build", started())
        .build();
    Painter::seeded(&env, draw, seed)
}

/// Writes the registry of the run under `dir`, naming `pid` as its engine and
/// the node it asks about at its terminal, when it asks.
fn registered(dir: &Path, pid: Pid, asking: Option<&str>) {
    let file = EngineProcessFile {
        schema_version: 1,
        engine_pid: pid,
        started_at: yunta_engine::lock::SystemProbe
            .started(pid)
            .unwrap_or(chrono::DateTime::UNIX_EPOCH),
        process_groups: Vec::new(),
        asking: asking.map(|node| Prompt {
            node: Some(NodeId::from(node)),
            since: chrono::DateTime::UNIX_EPOCH,
        }),
        words: None,
    };
    let path = yunta_engine::registry_path(dir);
    std::fs::create_dir_all(path.parent().expect("the registry's directory")).unwrap();
    std::fs::write(path, serde_json::to_vec(&file).unwrap()).unwrap();
}

/// A process that has come and gone, so its pid names nothing alive.
fn gone() -> Pid {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    child.wait().unwrap();
    Pid::try_from(child.id()).unwrap()
}

/// The engine a follower's frame is read with is the one the run's registry
/// names: gone, the run is stalled; asking, it needs a person. The engine that
/// drives the run reads itself alive, as `yunta run` draws it.
#[tokio::test]
async fn a_painter_reading_the_registry_calls_a_run_with_a_dead_engine_stalled() {
    let at = tempfile::tempdir().unwrap();
    let here = painter_of(EngineReading::This, at.path()).await;
    assert_eq!(here.reading().1.standing, Standing::Running);

    let dead = tempfile::tempdir().unwrap();
    registered(dead.path(), gone(), None);
    let painter = painter_of(
        EngineReading::Registry(yunta_api::RunRegistry::at(dead.path().to_path_buf())),
        at.path(),
    )
    .await;
    assert_eq!(painter.reading().1.standing, Standing::Stalled);

    let asking = tempfile::tempdir().unwrap();
    let this = Pid::try_from(std::process::id()).unwrap();
    registered(asking.path(), this, Some("build"));
    let painter = painter_of(
        EngineReading::Registry(yunta_api::RunRegistry::at(asking.path().to_path_buf())),
        at.path(),
    )
    .await;
    assert_eq!(painter.reading().1.standing, Standing::NeedsYou);
}
