//! A run's pane at work ends with the feed of its node, which alone scrolls there: it opens
//! at its end and stays there as it grows; a page down onto its last row goes to its end;
//! scrolled back it is the person's, keeps its rows in sight as more come and counts them
//! under its window, until `f` gives it back at its end with both pointers. On the design's
//! run, `implement` at work on its task.

use surco::components::{Keys, Laid, ScreenLaid};
use surco::doc::Block;
use surco::surface::Terminal;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::ScrolledTo;

use super::look;

/// Where the design's run stands at the target `board`: `implement` at work on its task,
/// eight calls made. Four calls on, it stands at [`FOUR_CALLS_ON`]; at [`CHECKED_AGAIN`] its
/// task was checked again in a session of its own, `implement` still at work.
const FOUR_CALLS_ON: u64 = 104;

/// See [`FOUR_CALLS_ON`].
const CHECKED_AGAIN: u64 = 112;

/// The rows a run's pane is laid out in over its feed: the window's least, and a few rows of
/// the loop's board above it.
const FEED_ROWS: usize = 8;

/// `screen`'s pane laid out in [`FEED_ROWS`] rows, what was laid out handed back as a
/// terminal hands it back: each row's text, the loop's board above the feed's window, and
/// what was laid out of the feed.
fn pane_laid_out(screen: &mut RunScreen) -> (Vec<String>, Laid) {
    let shown = screen.state().screen(&look());
    let paned = Terminal::on(look()).pane(&shown.main, FEED_ROWS);
    screen.laid(ScreenLaid {
        main: paned.laid,
        side: None,
        overlay: None,
    });
    let lines = paned.lines.iter().map(|line| line.text()).collect();
    (lines, paned.laid)
}

/// `strokes` pressed on `screen` one at a time, its pane laid out after each as a terminal
/// draws after each key: the pane as the last left it.
async fn pane_pressed(screen: &mut RunScreen, strokes: &[Stroke]) -> (Vec<String>, Laid) {
    let mut laid_out = pane_laid_out(screen);
    for stroke in strokes {
        screen.pressed(&[*stroke]).await;
        laid_out = pane_laid_out(screen);
    }
    laid_out
}

/// Where the pane of `screen`'s run stands.
fn pane_at(screen: &RunScreen) -> ScrolledTo {
    let pane = screen.state().pane().expect("a run's screen has a pane");
    pane.to()
}

/// The feed at the foot of `screen`'s pane: its newest row where what is handed of it
/// reaches it, as the pane draws it, cut at the pane's edge as a row is and one step in
/// where the feed stands one step under its board; and how many rows the whole feed holds.
fn feed_of(screen: &RunScreen) -> (Option<String>, usize) {
    let look = look();
    let shown = screen.state().screen(&look);
    let (feed, step) = match shown.main.doc.blocks().last() {
        Some(Block::Feed(feed)) => (feed, ""),
        Some(Block::Section(section)) => match section.blocks.as_slice() {
            [Block::Feed(feed)] => (feed, STEP),
            _ => panic!("the pane ends with a feed"),
        },
        _ => panic!("the pane ends with a feed"),
    };
    let newest = feed.rows.last().filter(|_| feed.after == 0);
    let cells = look.width.cells() - step.len();
    let cut = |row: &surco::components::FeedRow| {
        format!("{step}{}", row.line.cut(cells, look.glyphs).text())
    };
    (newest.map(cut), feed.before + feed.rows.len() + feed.after)
}

/// The cells a board's feed stands in from its pane's edge, one step under its tasks.
const STEP: &str = "  ";

/// The feed's window among `lines`, from the row that counts the rows above it.
fn window(lines: &[String]) -> &[String] {
    let at = lines.iter().position(|line| {
        let line = line.trim_start();
        line.starts_with("… ") && line.ends_with(" above")
    });
    &lines[at.expect("rows stand above the feed's window")..]
}

/// How many rows `line` counts under the feed's window, as `… {n} under` says it.
fn counted_under(line: &str) -> Option<usize> {
    line.trim_start()
        .strip_prefix("… ")?
        .strip_suffix(" under")?
        .parse()
        .ok()
}

/// Whether `screen` offers to give the lead back to the run, as it does only while the run
/// does not lead: `f` in its legend, and the row under its nodes that follows what it does.
fn offers_the_lead_back(screen: &RunScreen) -> (bool, bool) {
    let shown = screen.state().screen(&look());
    let in_the_legend = shown
        .legend
        .0
        .iter()
        .any(|binding| matches!(&binding.keys, Keys::Typed(key) if key == "f"));
    let side = shown.side.expect("the run's nodes beside its pane");
    let rows = Terminal::on(look()).lines(&side.region.doc);
    let under_the_nodes = rows
        .iter()
        .any(|row| row.text().contains("follow what it is doing"));
    (in_the_legend, under_the_nodes)
}

#[tokio::test]
async fn a_feed_at_its_end_stays_at_its_end_as_it_grows() {
    let mut screen = run::opened("board").await;
    let (lines, _) = pane_laid_out(&mut screen);
    let (newest, whole) = feed_of(&screen);
    assert_eq!(pane_at(&screen), ScrolledTo::End, "a feed opens at its end");
    assert_eq!(lines.last(), newest.as_ref(), "its newest row last");

    screen.went_on(FOUR_CALLS_ON).await;
    let (lines, _) = pane_laid_out(&mut screen);
    let (newest, grown) = feed_of(&screen);
    assert!(
        grown > whole,
        "the run's news grow the feed: {whole} then {grown}"
    );
    assert_eq!(pane_at(&screen), ScrolledTo::End);
    assert_eq!(
        lines.last(),
        newest.as_ref(),
        "still at its end once it grew"
    );
}

#[tokio::test]
async fn a_page_down_onto_a_feeds_last_row_goes_to_its_end_and_a_page_up_from_there_to_a_row() {
    let mut screen = run::opened("board").await;
    pane_pressed(&mut screen, &[Stroke::Home]).await;
    assert_eq!(pane_at(&screen), ScrolledTo::Row(0), "its first row");
    let (_, whole) = feed_of(&screen);
    for _ in 0..whole {
        if pane_at(&screen) == ScrolledTo::End {
            break;
        }
        pane_pressed(&mut screen, &[Stroke::PageDown]).await;
    }
    assert_eq!(
        pane_at(&screen),
        ScrolledTo::End,
        "a page down onto its last row"
    );

    screen.went_on(FOUR_CALLS_ON).await;
    let (lines, laid) = pane_laid_out(&mut screen);
    let (newest, _) = feed_of(&screen);
    assert_eq!(
        lines.last(),
        newest.as_ref(),
        "at its end, it grows in sight"
    );

    // A page up steps back from its end by the rows a window shows between its two counts.
    pane_pressed(&mut screen, &[Stroke::PageUp]).await;
    let between_the_counts = laid.page - 1;
    assert_eq!(
        pane_at(&screen),
        ScrolledTo::Row(laid.most - between_the_counts),
        "a page up from its end, at a row"
    );
}

/// `screen` with its feed scrolled back by the arrows from its end, laid out after each key:
/// the pane as the last key left it, and where it stands.
async fn scrolled_back(screen: &mut RunScreen) -> (Vec<String>, usize) {
    pane_laid_out(screen);
    let (back, _) = pane_pressed(screen, &[Stroke::Up, Stroke::Up, Stroke::Up]).await;
    let ScrolledTo::Row(place) = pane_at(screen) else {
        panic!("scrolled back to a row");
    };
    (back, place)
}

#[tokio::test]
async fn a_feed_scrolled_back_is_the_persons_and_keeps_its_rows_in_sight_as_it_grows() {
    let mut screen = run::opened("board").await;
    pane_laid_out(&mut screen);
    assert_eq!(
        offers_the_lead_back(&screen),
        (false, false),
        "the run leads"
    );
    let (back, place) = scrolled_back(&mut screen).await;
    assert_eq!(
        offers_the_lead_back(&screen),
        (true, true),
        "scrolled back, the feed is the person's"
    );
    let (_, whole) = feed_of(&screen);

    screen.went_on(CHECKED_AGAIN).await;
    let (later, _) = pane_laid_out(&mut screen);
    let (_, grown) = feed_of(&screen);
    assert!(
        grown > whole,
        "the run's news grow the feed: {whole} then {grown}"
    );
    assert_eq!(
        pane_at(&screen),
        ScrolledTo::Row(place),
        "the run's news leave it"
    );
    let (before, after) = (window(&back), window(&later));
    assert_eq!(
        before[..before.len() - 1],
        after[..after.len() - 1],
        "the rows in sight stand where they stood"
    );
    let under = |window: &[String]| window.last().and_then(|line| counted_under(line));
    assert_eq!(
        under(after),
        under(before).map(|under| under + grown - whole),
        "the rows that came counted under them"
    );
    assert_eq!(offers_the_lead_back(&screen), (true, true));
}

#[tokio::test]
async fn f_gives_a_feed_scrolled_back_to_the_run_at_its_end_with_both_pointers() {
    let mut screen = run::opened("board").await;
    scrolled_back(&mut screen).await;
    screen.went_on(CHECKED_AGAIN).await;

    screen.pressed(&[Stroke::Insert('f')]).await;
    let (lines, _) = pane_laid_out(&mut screen);
    let (newest, _) = feed_of(&screen);
    assert_eq!(
        pane_at(&screen),
        ScrolledTo::End,
        "`f` gives it back at its end"
    );
    assert_eq!(lines.last(), newest.as_ref(), "its newest row last");
    assert_eq!(
        offers_the_lead_back(&screen),
        (false, false),
        "the run leads again"
    );
    let trail = screen.state().screen(&look()).trail;
    assert_eq!(
        trail.last().map(String::as_str),
        Some("implement"),
        "its pointer on the node at work"
    );
}
