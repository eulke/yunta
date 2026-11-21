//! The scene a run's screen stands as, through `State` alone, on the cuts of the targets of
//! a run's screen: each cut makes the scene the state it stands in makes, the run leading
//! or the person pointing at a node. The cuts held here are the ones that stand with no
//! key a prompt in place takes, each opened by its target's name. A scene's pane is held
//! here where a frame does not show what it is made of: the title a pane opens with, and
//! the heading its feed stands under.

use surco::components::Keys;
use surco::doc::Block;
use surco::line::Line;
use surco::look::{Look, Width};
use surco::surface::Terminal;
use yunta_api::RunNews;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{AwaitedReading, Grounds, Scene, Standstill};

/// The scene `scene` stands as, by name, with what a card rests on and why a run stands
/// still.
fn named(scene: &Scene<'_>) -> &'static str {
    match scene {
        Scene::Work => "work",
        Scene::Board => "board",
        Scene::Findings => "findings",
        Scene::Round => "round",
        Scene::Desk => "desk",
        Scene::Card(_, Grounds::Shown) => "a card on what the gate shows",
        Scene::Card(_, Grounds::Failure) => "a card on a failure",
        Scene::Card(_, Grounds::Scope) => "a card on a scope",
        Scene::Card(_, Grounds::Departure) => "a card on a departure",
        Scene::Card(_, Grounds::Cap) => "a card on a cap",
        Scene::PullRequest => "pull request",
        Scene::Standstill(Standstill::Unanswered) => "a standstill no option answers here",
        Scene::Standstill(Standstill::NoRoom) => "a standstill for room",
        Scene::Standstill(Standstill::HalfDone) => "a standstill on what is left half done",
        Scene::Standstill(Standstill::Stopped) => "a standstill where it stopped",
        Scene::Standstill(Standstill::NoProcess) => "a standstill with no process",
        Scene::AskedElsewhere => "asked elsewhere",
        Scene::Receipt => "receipt",
        Scene::Ended => "ended",
        Scene::Node => "the node's own",
    }
}

/// The scene `screen` stands as, by name.
fn scene_of(screen: &RunScreen) -> Option<&'static str> {
    screen.state().scene().as_ref().map(named)
}

/// Each of `rows`, a target and the scene its row names, opened on its cut: the scene its
/// screen stands as, which says under it what the run has spent, whatever the scene but the
/// plan's desk, which heads itself and reaches the attention row.
async fn holds(rows: &[(&str, &str)]) {
    for (target, row) in rows {
        let screen = run::opened(target).await;
        assert_eq!(scene_of(&screen), Some(*row), "`{target}`");
        let meter = screen.state().screen(&Look::plain()).meter;
        let says_it = *row != named(&Scene::Desk);
        assert_eq!(
            meter.is_some(),
            says_it,
            "`{target}` says what the run spent"
        );
    }
}

/// Whether the legend of `screen` offers `key`.
fn offers(screen: &RunScreen, key: &str) -> bool {
    let legend = screen.state().screen(&Look::plain()).legend;
    legend
        .0
        .iter()
        .any(|binding| matches!(&binding.keys, Keys::Typed(typed) if typed == key))
}

#[tokio::test]
async fn a_node_at_work_stands_as_its_work_its_board_or_its_findings() {
    holds(&[
        ("work", "work"),
        ("work-moved", "work"),
        ("work-command", "work"),
        ("board", "board"),
        ("board-moved", "board"),
        ("board-tasks", "board"),
        ("findings", "findings"),
        ("findings-answered", "findings"),
        // The person points at a node at work: it stands as its work all the same.
        ("node-working", "work"),
        ("node-sent-back", "work"),
    ])
    .await;
}

#[tokio::test]
async fn work_not_started_or_standing_still_stands_as_its_node_at_work() {
    holds(&[
        ("created", "work"),
        ("wakes-alone", "work"),
        ("card-unread", "work"),
        // The answer awaiting its resume, and the pause not read yet, stand on the loop.
        ("still-answered", "board"),
        ("still-unread", "board"),
    ])
    .await;
}

#[tokio::test]
async fn a_node_the_person_points_at_stands_as_its_own() {
    holds(&[
        ("node-needs-you", "the node's own"),
        ("node-ended", "the node's own"),
        ("node-never-ran", "the node's own"),
        ("node-skipped", "the node's own"),
    ])
    .await;
}

#[tokio::test]
async fn what_a_run_parked_on_a_person_asks_stands_in_place() {
    holds(&[
        ("round", "round"),
        ("desk", "desk"),
        // The plan's gate counted no file: the plan leads. The last gate counted seven:
        // the change leads.
        ("ship", "a card on what the gate shows"),
        ("card", "a card on what the gate shows"),
        ("failure", "a card on a failure"),
        ("scope", "a card on a scope"),
        ("departure", "a card on a departure"),
        ("budget", "a card on a cap"),
        ("loop-cap", "a card on a cap"),
        ("pull-request", "pull request"),
        ("asked-elsewhere", "asked elsewhere"),
    ])
    .await;
}

#[tokio::test]
async fn a_run_with_nothing_to_answer_here_stands_still() {
    holds(&[
        ("standstill", "a standstill no option answers here"),
        ("no-room", "a standstill for room"),
        ("orphans", "a standstill on what is left half done"),
        ("stopped", "a standstill where it stopped"),
        ("stalled", "a standstill with no process"),
    ])
    .await;
}

/// The trail of `screen`: where the person stands.
fn trail(screen: &RunScreen) -> Vec<String> {
    screen.state().screen(&Look::plain()).trail
}

/// A run standing still for room: `u` opens upkeep over it, where collecting what runs left
/// makes room once confirmed, the run followed under it; `esc` comes back to the run, which
/// stands as it stood.
#[tokio::test]
async fn on_no_room_u_opens_upkeep_over_the_run_and_esc_comes_back_to_it() {
    let mut screen = run::opened("no-room").await;
    let on_the_run = trail(&screen);
    let run = screen.state().followed().run;
    assert!(offers(&screen, "u"), "the legend offers `u` on no room");
    screen.pressed(&[Stroke::Insert('u')]).await;
    assert_eq!(
        trail(&screen).last().map(String::as_str),
        Some(yunta_render::screen::UPKEEP)
    );
    assert_eq!(
        screen.state().followed().run,
        run,
        "the run is followed under it"
    );
    screen.pressed(&[Stroke::Decline]).await;
    assert_eq!(trail(&screen), on_the_run);
    assert_eq!(scene_of(&screen), Some("a standstill for room"));
}

#[tokio::test]
async fn a_run_that_ended_stands_as_its_receipt_or_its_end() {
    holds(&[
        ("receipt", "receipt"),
        ("reported", "receipt"),
        ("failed", "ended"),
        // The person points at the node its end names as the promotion comes.
        ("promoted", "ended"),
        ("broken", "ended"),
    ])
    .await;
}

#[tokio::test]
async fn a_decision_read_for_another_pause_of_the_run_is_not_the_one_it_waits_on() {
    let scope = run::opened("scope").await;
    let in_view = scope.state().followed_runs().in_view.as_deref();
    let Some(AwaitedReading::Read { pause, awaited }) =
        in_view.and_then(|in_view| in_view.awaited.clone())
    else {
        panic!("the scope's decision is read for its pause");
    };
    // The same run parked later, on its suite's failure: handed the scope's reading, it
    // stands as it does before its own decision is read, and is offered no answer of the
    // scope's.
    let mut failure = run::opened("failure").await;
    assert_eq!(scene_of(&failure), Some("a card on a failure"));
    let answered = offers(&failure, "enter");
    failure.handed(RunNews::Awaited(Some((pause, awaited))));
    assert_eq!(scene_of(&failure), Some("work"));
    let offered = ["enter", "x", "r"].map(|key| offers(&failure, key));
    assert_eq!(
        (answered, offered),
        (true, [false, false, false]),
        "a run whose decision is not read stands as its work, still, whose legend names no \
         answer and none of the run's acts"
    );
}

#[tokio::test]
async fn the_node_a_run_ended_on_shows_its_end_whoever_points_at_it() {
    // A broken run stands on its gate at its end: the person points at the node after it,
    // then around to the gate again.
    let mut broken = run::opened("broken").await;
    assert_eq!(scene_of(&broken), Some("ended"));
    broken.pressed(&[Stroke::Insert('j')]).await;
    assert_eq!(scene_of(&broken), Some("the node's own"));
    broken.pressed(&[Stroke::Insert('j')]).await;
    assert_eq!(scene_of(&broken), Some("ended"));
    // A failed run of one node: the person points at it.
    let mut failed = run::opened("failed").await;
    failed.pressed(&[Stroke::Insert('j')]).await;
    assert_eq!(scene_of(&failed), Some("ended"));
}

/// The title `screen`'s pane opens with: its parts, and the words at its far end.
fn titled(screen: &RunScreen) -> (Vec<String>, Option<String>) {
    let main = screen.state().screen(&Look::plain()).main;
    (main.title, main.edge)
}

/// The heading the feed `screen`'s pane ends with hangs from, where it ends with one, in
/// plain glyphs: right above it, or over it one step in, as a board's.
fn feed_heading(screen: &RunScreen) -> Option<String> {
    let main = screen.state().screen(&Look::plain()).main;
    let blocks = main.doc.blocks();
    match blocks {
        [.., Block::Heading(heading), Block::Feed(_)] => Some(heading.clone()),
        [.., Block::Section(section)] => match section.blocks.as_slice() {
            [Block::Feed(_)] => Some(section.title.text()),
            _ => None,
        },
        _ => None,
    }
}

/// `screen`'s pane drawn plain, `cells` wide and `rows` high, as a terminal lays it out:
/// each row's text.
fn pane_drawn(screen: &RunScreen, cells: usize, rows: usize) -> Vec<String> {
    let look = Look {
        width: Width::exact(cells),
        ..Look::plain()
    };
    let main = screen.state().screen(&look).main;
    let paned = Terminal::on(look).pane(&main, rows);
    paned.lines.iter().map(Line::text).collect()
}

#[tokio::test]
async fn a_node_at_work_titles_its_pane_by_its_kind_and_who_works_it() {
    let rows = [
        (
            "work",
            ["plan", "prompt"],
            Some("planner on mock/gpt-6-luna"),
        ),
        ("work-command", ["tests", "check"], Some("baseline_compare")),
        ("created", ["build", "bash"], None),
        (
            "board",
            ["implement", "loop"],
            Some("executor on mock/gpt-6-luna"),
        ),
        (
            "board-tasks",
            ["implement", "loop"],
            Some("executor on mock/mock-model"),
        ),
    ];
    for (target, title, works_it) in rows {
        let screen = run::opened(target).await;
        assert_eq!(
            titled(&screen),
            (
                title.map(str::to_string).to_vec(),
                works_it.map(str::to_string)
            ),
            "`{target}`"
        );
    }
}

#[tokio::test]
async fn a_nodes_feed_stands_under_the_count_of_its_open_attempts_calls_once_it_opened() {
    let rows = [
        ("work", Some("what it is doing ; 3 calls")),
        ("work-moved", Some("what it is doing ; 8 calls")),
        ("work-command", Some("what it is doing ; 0 calls")),
        ("board", Some("what it is doing ; 8 calls")),
        ("board-tasks", Some("what it is doing ; 4 calls")),
        // Nothing ran yet: what runs first stands, over a feed with no row and no heading.
        ("created", None),
    ];
    for (target, heading) in rows {
        let screen = run::opened(target).await;
        assert_eq!(feed_heading(&screen).as_deref(), heading, "`{target}`");
    }
}

/// With two tasks at work, the loop's pane holds each with its proof and its scope, the
/// paths its own sessions wrote marked, over the loop's one feed, whose calls each name
/// their task; in rows too few for all, the scopes give way and the feed's newest row stays
/// the pane's last.
#[tokio::test]
async fn a_loop_with_two_tasks_at_work_holds_each_task_over_one_feed_naming_their_tasks() {
    let screen = run::opened("board-tasks").await;
    let whole = pane_drawn(&screen, 78, 40);
    for row in [
        "> task-1  Write the greeting ; attempt 1",
        "  by      test -f greeting.txt",
        "  its scope",
        "  + greeting.txt",
        "> task-2  Write the farewell ; attempt 1",
        "  + farewell.txt",
        "  write   task-1  greeting.txt",
        "  write   task-2  farewell.txt",
    ] {
        assert!(
            whole.iter().any(|drawn| drawn == row),
            "no `{row}` in {whole:#?}"
        );
    }
    let calls = whole.iter().filter(|row| {
        ["  read ", "  write "]
            .iter()
            .any(|verb| row.starts_with(verb))
    });
    for call in calls {
        assert!(
            call.contains("task-1") || call.contains("task-2"),
            "`{call}` names no task"
        );
    }
    let short = pane_drawn(&screen, 78, 14);
    assert!(
        !short.iter().any(|row| row == "  its scope"),
        "a scope gives way before the feed: {short:#?}"
    );
    assert_eq!(short.last(), whole.last(), "{short:#?}");
}

/// The node `screen`'s trail names after the run's handle.
fn named_in_the_trail(screen: &RunScreen) -> Option<String> {
    let trail = screen.state().screen(&Look::plain()).trail;
    trail.get(2).cloned()
}

/// `screen` with the person pointing at `node`, pressed `j` until the trail names it.
async fn pointing_at(screen: &mut RunScreen, node: &str) {
    for _ in 0..32 {
        if named_in_the_trail(screen).as_deref() == Some(node) {
            return;
        }
        screen.pressed(&[Stroke::Insert('j')]).await;
    }
    panic!("`j` never reached `{node}`");
}

/// The rows of the nodes beside `screen`'s pane, plain.
fn nodes_beside(screen: &RunScreen) -> Vec<String> {
    let side = screen.state().screen(&Look::plain()).side;
    let side = side.expect("a run's screen lists its nodes beside its pane");
    side.region
        .doc
        .blocks()
        .iter()
        .flat_map(|block| match block {
            Block::Rows(rows) => rows.iter().map(Line::text).collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// A node that ended after three attempts, pointed at once the run finished: its own scene's
/// feed lists every call of every attempt, 13, 7 and 3 as the log holds them, each attempt
/// named where it opens, with no row standing above its window in a pane tall enough.
#[tokio::test]
async fn a_finished_nodes_feed_lists_every_call_of_every_attempt_each_named_where_it_opens() {
    let mut screen = run::opened("receipt").await;
    pointing_at(&mut screen, "implement").await;
    assert_eq!(scene_of(&screen), Some("the node's own"));
    let pane = pane_drawn(&screen, 84, 200);
    let opened: Vec<usize> = (1..=3)
        .map(|attempt| {
            let opens = format!("running: attempt {attempt}");
            pane.iter()
                .position(|row| *row == opens)
                .unwrap_or_else(|| panic!("no `{opens}` in {pane:#?}"))
        })
        .collect();
    assert!(opened.is_sorted(), "{pane:#?}");
    // A call is its verb in the column the widest, `submit`, and two spaces make.
    let calls = pane.iter().filter(|row| {
        ["read", "edit", "write", "run", "call", "submit"]
            .iter()
            .any(|verb| row.starts_with(&format!("{verb:<8}")))
    });
    assert_eq!(calls.count(), 13 + 7 + 3, "{pane:#?}");
    assert!(!pane.iter().any(|row| row.ends_with(" above")), "{pane:#?}");
}

/// A reviewer that ended having posted findings, pointed at once the run finished: what it
/// did is its findings, which it shows as the findings scene does.
#[tokio::test]
async fn a_node_that_ended_having_posted_findings_shows_them_as_the_findings_scene() {
    let mut screen = run::opened("receipt").await;
    pointing_at(&mut screen, "review@reviewer").await;
    assert_eq!(scene_of(&screen), Some("findings"));
}

/// The gate that sent the run back, pointed at: its row keeps the mark of a node sent back
/// under the pointer, and its own scene says it stands as it ended, by that mark's word.
#[tokio::test]
async fn the_node_sent_back_is_pointed_with_its_mark_and_says_it_stands_as_it_ended() {
    let mut screen = run::opened("node-sent-back").await;
    pointing_at(&mut screen, "approve-plan").await;
    assert_eq!(scene_of(&screen), Some("the node's own"));
    let nodes = nodes_beside(&screen);
    assert!(
        nodes.iter().any(|row| row == "* @ approve-plan"),
        "{nodes:#?}"
    );
    let pane = pane_drawn(&screen, 78, 24);
    assert!(
        pane.iter().any(|row| row == "  stands       rerouted"),
        "{pane:#?}"
    );
}

#[path = "scenes/asked_elsewhere.rs"]
mod asked_elsewhere;

#[path = "scenes/child.rs"]
mod child;

#[path = "scenes/findings.rs"]
mod findings;

#[path = "scenes/pull_request.rs"]
mod pull_request;

#[path = "scenes/receipt.rs"]
mod receipt;

#[path = "scenes/still.rs"]
mod still;

#[path = "scenes/successor.rs"]
mod successor;
