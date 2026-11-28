use std::path::Path;

use surco::words::Standing;
use yunta_core::events::{
    AgentMessagePayload, AgentMessageType, SessionEvent, ToolAction, ToolTarget,
};
use yunta_core::events::{ChildEvent, EventPayload, RunEvent};
use yunta_core::events::{
    ChildRunCreatedPayload, CommandExit, Failure, NodeFailedPayload, NodeStartedPayload,
    RunFinishedPayload, RunMetrics, TokenUsage,
};
use yunta_core::{ContentHash, Manifest, NodeId, Pid};
use yunta_engine::lock::OwnerProbe;
use yunta_engine::EngineProcessFile;
use yunta_replay::registry::Prompt;
use yunta_testkit_core::{Captured, FixedClock, Log};

use super::*;
use crate::surface::region::Region;
use crate::surface::{Delivery, EngineReading, Screen, Watched};
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
    crate::surface::frozen("name: drawn\nnodes:\n  - { id: build, kind: bash, run: \"true\" }\n")
}

/// The `run_created` a run's log opens with, which is what makes it a run
/// that moves rather than one not yet born: of the run a promotion handed the
/// work to, where `promoted_from` names the run that handed it.
fn born(promoted_from: Option<&str>) -> EventPayload {
    EventPayload::Run(RunEvent::Created(yunta_core::events::RunCreatedPayload {
        checkout: None,
        manifest_hash: yunta_core::sha256_hex(b"manifest"),
        inputs: Default::default(),
        mode: Default::default(),
        promoted_from: promoted_from.map(RunId::from),
        yunta_schema: None,
        base_branch: "main".to_string(),
        base_commit: "0123456789abcdef0123456789abcdef01234567".into(),
        environment: None,
        left_out: Vec::new(),
        opens_on_base: false,
    }))
}

/// The store the surface reads a run's log from, opened under `at`.
async fn store(at: &Path) -> EventLog {
    EventLog::open(&at.join(STORE))
        .await
        .expect("a store opens under the test's own directory")
}

/// The file of the store [`store`] opens under a test's directory.
const STORE: &str = "events.db";

/// A painter on `seed`, the run's log as the surface opened with it, that
/// reads the rest of it from `log`, reading the process driving the run the
/// way `engine` says and putting the run on `draw`.
fn painter(log: &EventLog, engine: EngineReading, draw: Draw, seed: Vec<StoredEvent>) -> Painter {
    let manifest = manifest();
    let run_id = RunId::from(DRAWN);
    let env = SurfaceEnv {
        run_id: &run_id,
        manifest: &manifest,
        log,
        clock: Arc::new(FixedClock),
        delivery: Delivery::Lines { reason: "a test" },
        glyphs: Glyphs::Ascii,
        engine,
    };
    Painter::seeded(&env, draw, seed)
}

/// The append-only surface, its lines written into `written`.
fn lines(written: &Captured) -> Draw {
    Draw::Lines(crate::surface::lines::Lines::open(
        Box::new(written.clone()),
        "a test",
    ))
}

/// A painter on a run whose `build` node has started, reading the process
/// driving it the way `engine` says, and drawing lines nobody reads.
async fn painter_of(engine: EngineReading, at: &Path) -> Painter {
    let draw = lines(&Captured::default());
    let seed = Log::for_run(DRAWN)
        .event(born(None))
        .node("build", started())
        .build();
    painter(&store(at).await, engine, draw, seed)
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
    assert_eq!(here.folded.run().frame().standing, Standing::Running);

    let dead = tempfile::tempdir().unwrap();
    registered(dead.path(), gone(), None);
    let painter = painter_of(
        EngineReading::Registry(yunta_api::RunRegistry::at(dead.path().to_path_buf())),
        at.path(),
    )
    .await;
    assert_eq!(painter.folded.run().frame().standing, Standing::Stalled);

    let asking = tempfile::tempdir().unwrap();
    let this = Pid::try_from(std::process::id()).unwrap();
    registered(asking.path(), this, Some("build"));
    let painter = painter_of(
        EngineReading::Registry(yunta_api::RunRegistry::at(asking.path().to_path_buf())),
        at.path(),
    )
    .await;
    assert_eq!(painter.folded.run().frame().standing, Standing::NeedsYou);
}

/// How many events the run a redraw is counted on holds: enough that a
/// redraw that read its log whole would read a thousand events where it
/// should read none.
const MANY: usize = 1_000;

/// A call `build` makes, reading the file numbered `file`: what most of a
/// working run's log is.
fn call(file: usize) -> EventPayload {
    EventPayload::Session(SessionEvent::Message(AgentMessagePayload {
        message_type: AgentMessageType::ToolUse,
        tool_name: None,
        action: Some(ToolAction::Read),
        target: Some(ToolTarget::of_path(Path::new(&format!(
            "src/part_{file}.rs"
        )))),
        session: None,
        input_tokens: None,
        output_tokens: None,
        cached_input_tokens: None,
        note: None,
        text: None,
    }))
}

/// The log of a run of `events` events, a second apart: created, its `build`
/// started, and the rest calls `build` made; with the call it makes next.
fn working(events: usize) -> (Vec<StoredEvent>, StoredEvent) {
    let log = (0..events - 1).fold(
        Log::for_run(DRAWN)
            .event(born(None))
            .node("build", started()),
        |log, file| log.after(1).node("build", call(file)),
    );
    let mut log = log.build();
    let next = log.pop().expect("the call the run makes next");
    (log, next)
}

/// The region of a run on `terminal`, its lines going to a scrollback nobody
/// reads.
fn region(terminal: &Watched) -> Draw {
    let region = Region::open(
        Screen::immediate(terminal.clone()),
        Glyphs::Ascii,
        surco::paint::Paint::Plain,
        Box::new(Captured::default()),
    )
    .expect("the region's row template parses");
    Draw::Live(Box::new(region))
}

/// A redraw with no news reads no event of the run, however long its log, and
/// a piece of news reads only itself: the run is folded as each event comes,
/// and a redraw works out only what the clock adds to it. Counted through the
/// events the painter's holding of the run hands to be folded, never a timer.
#[tokio::test]
async fn a_redraw_with_no_news_reads_no_event_and_a_piece_of_news_only_itself() {
    let at = tempfile::tempdir().unwrap();
    let terminal = Watched::sized(24, 80);
    let (log, next) = working(MANY);
    let store = store(at.path()).await;
    let mut painter = painter(&store, EngineReading::This, region(&terminal), log);
    assert_eq!(
        painter.folded.run().frame().standing,
        Standing::Running,
        "a run at work, its log read whole"
    );
    let read = painter.folded.events_read();

    painter.beat().await;
    painter.beat().await;
    assert_eq!(
        painter.folded.events_read() - read,
        0,
        "two redraws with no news between them read no event"
    );
    assert!(
        terminal.shown().contains("build"),
        "the region draws the run at work: {}",
        terminal.shown()
    );

    painter
        .take(std::iter::once(Beat::Event(Box::new(next))))
        .await;
    assert_eq!(
        painter.folded.events_read() - read,
        1,
        "a piece of news reads only itself"
    );
}

/// A node of the run that failed, with what it printed kept: its line names
/// the run, where the rest of what it printed is read.
fn failed() -> EventPayload {
    EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
        Failure::exited(CommandExit {
            origin: None,
            code: 1,
            tail: vec!["boom".to_string()],
            output: Some(ContentHash::sha256(b"boom")),
        }),
        TokenUsage::default(),
    )))
}

/// The log of a run whose `build` failed before it closed promoted, and the
/// log of its successor, whose `build` failed too.
fn promotion() -> (Vec<StoredEvent>, Vec<StoredEvent>) {
    let predecessor = Log::for_run(DRAWN)
        .event(born(None))
        .node("build", started())
        .node("build", failed())
        .event(closed(TerminalState::Promoted))
        .build();
    let successor = Log::for_run(OTHER.as_str())
        .event(born(Some(DRAWN)))
        .node("build", started())
        .node("build", failed())
        .build();
    (predecessor, successor)
}

/// The events of `log` as the queue carries them.
fn queued(log: &[StoredEvent]) -> impl Iterator<Item = Beat> + '_ {
    log.iter()
        .cloned()
        .map(|event| Beat::Event(Box::new(event)))
}

/// A promotion folded while a prompt holds the terminal leaves owed the
/// lines its predecessor's last events earned, as it leaves the successor's:
/// once the prompt ends they go out, the predecessor's first, each naming its
/// own run.
#[tokio::test]
async fn a_predecessors_lines_owed_under_a_prompt_go_out_before_its_successors_each_under_its_run()
{
    let at = tempfile::tempdir().unwrap();
    let written = Captured::default();
    let (predecessor, successor) = promotion();
    let seed = predecessor[..2].to_vec();
    let store = store(at.path()).await;
    let mut painter = painter(&store, EngineReading::This, lines(&written), seed);
    let opened = written.text();

    painter.stand(crate::surface::turns::Standing::StoodDown);
    // The predecessor's last events and the successor's first, as the engine
    // hands them; then the successor's log, as the next pass takes it.
    painter
        .take(queued(&predecessor[2..]).chain(queued(&successor[..1])))
        .await;
    painter.take(queued(&successor)).await;
    assert_eq!(
        written.text(),
        opened,
        "nothing goes out while a prompt holds the terminal"
    );

    painter.stand(crate::surface::turns::Standing::Drawing);
    let since = written.text();
    let named: Vec<&str> = since[opened.len()..]
        .lines()
        .filter(|line| line.contains("yunta status"))
        .collect();
    let drawn = RunId::from(DRAWN);
    assert!(
        matches!(
            named.as_slice(),
            [first, second] if first.contains(drawn.handle()) && second.contains(OTHER.handle())
        ),
        "the predecessor's failure, then the successor's, each naming its own run: {named:?}\n{since}"
    );
}

/// A painter born for a run just created begins from the log it is handed and
/// folds what the engine hands it after: it reads no store to be born, to beat,
/// or to take the run's first append, so its birth costs the run's start no
/// read. The store's file, removed once the surface's log is open, is the
/// probe: a read of the store makes it again.
#[tokio::test]
async fn a_painter_born_for_a_run_just_created_reads_no_store_to_begin() {
    let at = tempfile::tempdir().unwrap();
    let log = store(at.path()).await;
    let file = at.path().join(STORE);
    std::fs::remove_file(&file).expect("the store's file, made as the log opened");
    let created = Log::for_run(DRAWN)
        .event(born(None))
        .node("build", started())
        .build();

    let draw = lines(&Captured::default());
    let mut painter = painter(&log, EngineReading::This, draw, created[..1].to_vec());
    painter.beat().await;
    painter.take(queued(&created[1..])).await;
    painter.beat().await;
    assert!(
        !file.exists(),
        "the painter read the store to begin: {}",
        file.display()
    );
    assert_eq!(
        painter.folded.head(),
        Some(created[1].seq),
        "it holds the run as far as it was handed"
    );
}
