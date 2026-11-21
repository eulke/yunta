//! What holds on a run's screen while its run moves, through `State` alone, on the design's
//! run let go one event of its log at a time, each folded as its following hands it: a feed
//! scrolled back by the arrows keeps the rows in sight where they stand as calls come under
//! them, and counts them there; a decision that comes while the person points at another node
//! takes no key from them, the attention row alone saying it; half a note typed on a card
//! stays through news of its pause; an open palette keeps the row it points at. With no event,
//! two drawings a second apart differ only in their times, and each time is the log's, to the
//! second (`times`). A held prompt never meets its run read whole again, since the application
//! keeps one following, which reads a run's screen whole once (`one_following`).
//!
//! Reached by: `crates/ui/src/state.rs`, `crates/ui/src/scroll.rs`,
//! `crates/ui/src/stage/run.rs`, `crates/ui/src/stage/run/feed.rs`,
//! `crates/ui/src/stage/run/in_place.rs`, `crates/ui/src/stage/run/scene.rs`,
//! `crates/ui/src/dialog/palette.rs`, `crates/render/src/watched/nodes.rs`,
//! `crates/render/src/watched/spent.rs`

#[path = "holding/one_following.rs"]
mod one_following;
#[path = "holding/times.rs"]
mod times;

use surco::components::{CardBody, ScreenLaid};
use surco::doc::Block;
use surco::glyphs::Glyphs;
use surco::line::Line;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_core::events::{AgentMessageType, EventPayload, SessionEvent, StoredEvent};
use yunta_storage::Storage;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Grounds, Input, Operation, Request, Scene, ScrolledTo, State};

/// Where the design's run stands once `implement`, at work at the target `board`, made four
/// calls more: a usage, then the four calls, an event each.
const FOUR_CALLS_ON: u64 = 104;

/// Where the design's run parks on the scope `implement`'s task asks for, a decision with a
/// card of its own.
const THE_SCOPE_ASKED: u64 = 118;

/// Where the design's run stands once its last gate, parked on at the target `ship`, logs
/// that it waits on the person: news of the same pause.
const THE_GATE_WAITS: u64 = 264;

/// Where the design's run stands once `plan`, at work at the target `palette`, made six more
/// moves of its own: a usage, three calls, a note and a call.
const PLAN_SIX_ON: u64 = 48;

/// The rows a run's pane is laid out in over its feed: the window's least, and a few rows of
/// the loop's board above it.
const FEED_ROWS: usize = 8;

/// The cells inside the margins of the design's widest screen, 120 columns.
const WIDEST: usize = 118;

/// The look a pane's rows are laid out in: Unicode, unpainted, 80 cells, still.
fn look() -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(80),
        motion: Motion::Still,
    }
}

/// The look a whole screen is drawn in, `cells` wide, its spinner turning as a look that
/// moves turns it.
fn moving(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: Motion::Moves,
    }
}

/// The design's run's log in `screen`'s store, as far as it was let go.
fn logged(screen: &RunScreen) -> Vec<StoredEvent> {
    let run = screen.state().followed().run.expect("a run's screen");
    let storage = Storage::open(&screen.store()).expect("the cut's log");
    storage.events_for_run(&run).expect("the run's log")
}

/// The seq `screen`'s run stands at in its store: where its log was cut, or let go to.
fn head(screen: &RunScreen) -> u64 {
    let last = logged(screen).last().map(|event| event.seq.get());
    last.expect("the run's log holds its first event")
}

/// Whether the event at `seq` of `screen`'s run is a call its session made, as its log holds
/// it.
fn a_call_at(screen: &RunScreen, seq: u64) -> bool {
    let events = logged(screen);
    let event = events.iter().find(|event| event.seq.get() == seq);
    let event = event.expect("the event let go is on the log");
    matches!(
        event.payload(),
        Some(EventPayload::Session(SessionEvent::Message(message)))
            if message.message_type == AgentMessageType::ToolUse
    )
}

/// `screen`'s pane laid out in [`FEED_ROWS`] rows, what was laid out handed back as a
/// terminal hands it back: each row's text.
fn pane_laid_out(screen: &mut RunScreen) -> Vec<String> {
    let shown = screen.state().screen(&look());
    let paned = Terminal::on(look()).pane(&shown.main, FEED_ROWS);
    screen.laid(ScreenLaid {
        main: paned.laid,
        side: None,
        overlay: None,
    });
    paned.lines.iter().map(Line::text).collect()
}

/// Where the pane of `screen`'s run stands.
fn pane_at(screen: &RunScreen) -> ScrolledTo {
    let pane = screen.state().pane().expect("a run's screen has a pane");
    pane.to()
}

/// The feed's window among `lines`, from the row that counts the rows above it.
fn window(lines: &[String]) -> &[String] {
    let at = lines.iter().position(|line| {
        let line = line.trim_start();
        line.starts_with("… ") && line.ends_with(" above")
    });
    &lines[at.expect("rows stand above the feed's window")..]
}

/// The rows in sight in the feed's window among `lines`, the rows it counts above and under
/// them standing at its edges: all but the last, which counts the rows under.
fn in_sight(lines: &[String]) -> Vec<String> {
    let window = window(lines);
    window[..window.len() - 1].to_vec()
}

/// How many rows the feed's window among `lines` counts under it, as `… {n} under` says it.
fn counted_under(lines: &[String]) -> usize {
    let last = window(lines).last().expect("a window has rows");
    let counted = last.trim_start().strip_prefix("… ");
    let counted = counted.and_then(|counted| counted.strip_suffix(" under"));
    let counted = counted.and_then(|counted| counted.parse().ok());
    counted.unwrap_or_else(|| panic!("the window's last row counts the rows under: {lines:#?}"))
}

#[tokio::test]
async fn a_feed_scrolled_back_keeps_its_rows_in_sight_as_calls_come_one_at_a_time_under_them() {
    let mut screen = run::opened("board").await;
    pane_laid_out(&mut screen);
    for _ in 0..3 {
        screen.pressed(&[Stroke::Up]).await;
        pane_laid_out(&mut screen);
    }
    let ScrolledTo::Row(place) = pane_at(&screen) else {
        panic!("the arrows scroll the feed back to a row");
    };
    let back = pane_laid_out(&mut screen);
    let (sight, under) = (in_sight(&back), counted_under(&back));

    let at = head(&screen);
    let mut calls = 0;
    for seq in at + 1..=FOUR_CALLS_ON {
        screen.followed_on_to(seq).await;
        calls += usize::from(a_call_at(&screen, seq));
        let later = pane_laid_out(&mut screen);
        assert_eq!(
            pane_at(&screen),
            ScrolledTo::Row(place),
            "at {seq}: the run's news leave the feed where the person put it"
        );
        assert_eq!(
            in_sight(&later),
            sight,
            "at {seq}: the rows in sight stand where they stood"
        );
        assert_eq!(
            counted_under(&later),
            under + calls,
            "at {seq}: each call let go is counted under them: {later:#?}"
        );
    }
    assert_eq!(calls, 4, "four calls were let go");
}

/// The node `screen`'s trail names after the run's handle.
fn named_in_the_trail(screen: &RunScreen) -> Option<String> {
    let trail = screen.state().screen(&Look::plain()).trail;
    trail.get(2).cloned()
}

/// `screen` with the person pointing at `node`, `j` pressed until the trail names it.
async fn pointing_at(screen: &mut RunScreen, node: &str) {
    for _ in 0..32 {
        screen.pressed(&[Stroke::Insert('j')]).await;
        if named_in_the_trail(screen).as_deref() == Some(node) {
            return;
        }
    }
    panic!("`j` never reached `{node}`");
}

/// The attention row of `state`'s screen as a terminal draws it at the design's widest; none
/// where none stands.
fn attention_row(state: &State) -> Option<String> {
    let attention = state.screen(&moving(WIDEST)).attention?;
    let row = Terminal::on(moving(WIDEST)).attention(&attention);
    Some(row.text().trim_end().to_string())
}

/// The keys `state`'s legend says, as a terminal spells them.
fn legend(state: &State) -> String {
    let legend = state.screen(&Look::plain()).legend;
    legend.spelled(Glyphs::Unicode)
}

/// What `stroke` does on `state`, branched: where the person stands after it, the scene
/// there, and each piece of work it asks for.
fn what_it_does(state: &State, stroke: Stroke) -> (Vec<String>, String, Vec<Operation>) {
    let mut state = state.clone();
    let asked = state.update(Input::Stroke(stroke));
    let operations = asked.into_iter().filter_map(|request| match request {
        Request::Perform(_, operation) => Some(operation),
        _ => None,
    });
    let trail = state.screen(&Look::plain()).trail;
    let scene = format!("{:?}", state.scene());
    (trail, scene, operations.collect())
}

/// The keys a card in place takes, which mean something else on a node's own scene: `enter`
/// answers there, the numbers, `tab` and `j` choose, and the arrows move what stands above it.
const A_CARDS_KEYS: [Stroke; 7] = [
    Stroke::Enter,
    Stroke::Insert('1'),
    Stroke::Insert('2'),
    Stroke::Tab,
    Stroke::Insert('j'),
    Stroke::Up,
    Stroke::Down,
];

#[tokio::test]
async fn a_decision_that_comes_while_the_person_points_another_node_changes_no_keys_meaning() {
    let mut screen = run::opened("board-moved").await;
    pointing_at(&mut screen, "review@reviewer").await;
    let before = screen.state().clone();
    for seq in head(&screen) + 1..=THE_SCOPE_ASKED {
        screen.followed_on_to(seq).await;
    }
    assert_eq!(head(&screen), THE_SCOPE_ASKED, "the run's news landed");
    let after = screen.state().clone();
    assert!(
        matches!(after.scene(), Some(Scene::Node)),
        "the node the person points at stands as its own: {:?}",
        after.scene()
    );
    assert_eq!(
        named_in_the_trail(&screen).as_deref(),
        Some("review@reviewer")
    );
    let main = after.screen(&Look::plain()).main;
    assert!(
        !matches!(main.doc.blocks().last(), Some(Block::Card(_))),
        "no card stands in the person's pane"
    );
    assert_eq!(legend(&after), legend(&before), "the same keys");
    assert_eq!(
        attention_row(&after).as_deref(),
        Some("◆ Q78GRR needs you: a task asks to touch a path outside its scope  · a go there"),
        "the attention row names the run's own decision"
    );
    for stroke in A_CARDS_KEYS {
        assert_eq!(
            what_it_does(&after, stroke),
            what_it_does(&before, stroke),
            "{stroke:?} does what it did before the decision came"
        );
    }

    screen.pressed(&[Stroke::Insert('a')]).await;
    assert!(
        matches!(screen.state().scene(), Some(Scene::Card(_, Grounds::Scope))),
        "`a` goes to the card: {:?}",
        screen.state().scene()
    );
}

/// The words typed on the card standing at the foot of `screen`'s pane, before its cursor,
/// where a line is typed on it.
fn typed_on_the_card(screen: &RunScreen) -> Option<String> {
    let main = screen.state().screen(&Look::plain()).main;
    let Some(Block::Card(card)) = main.doc.blocks().last() else {
        return None;
    };
    match &card.body {
        CardBody::Saying { typed, .. } => Some(typed.before.clone()),
        CardBody::Choosing { .. } | CardBody::Round(_) => None,
    }
}

#[tokio::test]
async fn half_a_note_typed_on_a_card_stays_as_news_of_its_pause_lands() {
    let mut screen = run::opened("ship").await;
    let half = "ship it after the release";
    screen.pressed(&[Stroke::Insert('n')]).await;
    let typing: Vec<Stroke> = half.chars().map(Stroke::Insert).collect();
    screen.pressed(&typing).await;
    assert_eq!(typed_on_the_card(&screen).as_deref(), Some(half));

    screen.followed_on_to(THE_GATE_WAITS).await;
    assert_eq!(head(&screen), THE_GATE_WAITS, "the pause's news landed");
    assert!(
        matches!(screen.state().scene(), Some(Scene::Card(_, Grounds::Shown))),
        "{:?}",
        screen.state().scene()
    );
    assert_eq!(
        typed_on_the_card(&screen).as_deref(),
        Some(half),
        "news of the same pause leave what was typed"
    );
}

/// The palette standing over `state`: each of its rows by what it does, and the one it points
/// at.
fn palette(state: &State) -> (Vec<String>, usize) {
    let overlay = state.screen(&Look::plain()).overlay;
    let overlay = overlay.expect("the palette stands over the run");
    let listed = overlay.doc.blocks().iter().find_map(|block| match block {
        Block::ChoiceList(list) => list.choices.as_ref().map(|choices| {
            let rows = choices.rows.iter().map(|row| row.name.clone());
            (rows.collect(), choices.at)
        }),
        _ => None,
    });
    listed.expect("the palette lists rows")
}

#[tokio::test]
async fn an_open_palette_keeps_the_row_it_points_at_as_the_runs_news_land() {
    let mut screen = run::opened("palette").await;
    screen
        .pressed(&[Stroke::Down, Stroke::Down, Stroke::Down])
        .await;
    let (rows, at) = palette(screen.state());
    assert_eq!(at, 3, "{rows:#?}");

    for seq in head(&screen) + 1..=PLAN_SIX_ON {
        screen.followed_on_to(seq).await;
        let (now, pointed) = palette(screen.state());
        assert_eq!(
            (now[pointed].as_str(), pointed),
            (rows[at].as_str(), at),
            "at {seq}: the row pointed at stays"
        );
    }
    assert_eq!(head(&screen), PLAN_SIX_ON, "the run's news landed");
}
