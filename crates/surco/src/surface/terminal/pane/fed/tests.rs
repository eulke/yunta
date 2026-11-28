//! A feed one step under the blocks above it, hanging from its section's title, and what
//! stands above it said in fewer rows as its region says how: the first abridgement that
//! leaves the window its least, else the last, kept whole where the pane stands close.

use crate::components::{Feed, FeedRow, Region, Section, Sight};
use crate::doc::{Block, Doc};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::surface::Terminal;

/// A terminal whose panes are `cells` wide, in ASCII and unpainted.
fn terminal(cells: usize) -> Terminal {
    Terminal::on(Look {
        width: Width::exact(cells),
        ..Look::plain()
    })
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
}

/// A block of rows, one a line.
fn rows(lines: &[&str]) -> Block {
    Block::Rows(lines.iter().map(|line| Line::new().plain(*line)).collect())
}

/// A task's own row.
fn task() -> Block {
    rows(&["task-1  Write the greeting"])
}

/// How the task is proved, one step under its row.
fn proof() -> Block {
    rows(&["  proved   red", "  by      test -f greeting.txt"])
}

/// The task's scope, one step under its row.
fn scope() -> Block {
    rows(&["  its scope", "  + greeting.txt", "  + farewell.txt"])
}

/// A feed of `count` calls at its end, under its heading one step in.
fn calls(count: usize) -> Block {
    let feed = Feed {
        rows: (1..=count)
            .map(|at| Line::new().plain(format!("read    file-{at}.rs")))
            .map(FeedRow::of)
            .collect(),
        before: 0,
        after: 0,
    };
    Block::Section(Section {
        lead: None,
        title: Line::new().push(Tone::Muted, "what it is doing"),
        blocks: vec![Block::Feed(feed)],
    })
}

/// A loop's region: its task, its proof and its scope over its feed of `count` calls, said
/// again without the scope, then with the proof right under the task's row, then with the
/// task's row alone, at the feed's end.
fn board(count: usize) -> Region {
    let doc = Doc::new()
        .with(task())
        .with(proof())
        .with(scope())
        .with(calls(count));
    let close = rows(&[
        "task-1  Write the greeting",
        "  proved   red",
        "  by      test -f greeting.txt",
    ]);
    Region::new(doc)
        .abridged(vec![vec![task(), proof()], vec![close], vec![task()]])
        .in_sight(Sight::at_the_end())
}

#[test]
fn a_feed_in_a_section_stands_one_step_under_its_title() {
    let paned = terminal(40).pane(&board(2), 20);
    assert_eq!(
        texts(&paned.lines),
        [
            "task-1  Write the greeting",
            "",
            "  proved   red",
            "  by      test -f greeting.txt",
            "",
            "  its scope",
            "  + greeting.txt",
            "  + farewell.txt",
            "",
            "  what it is doing",
            "  read    file-1.rs",
            "  read    file-2.rs",
        ]
    );
}

#[test]
fn what_stands_above_a_feed_says_itself_in_the_first_abridgement_that_leaves_the_window_its_least()
{
    // The scope gives way first: its rows are the window's.
    let paned = terminal(40).pane(&board(20), 11);
    assert_eq!(
        texts(&paned.lines),
        [
            "task-1  Write the greeting",
            "",
            "  proved   red",
            "  by      test -f greeting.txt",
            "",
            "  what it is doing",
            "  ... 16 above",
            "  read    file-17.rs",
            "  read    file-18.rs",
            "  read    file-19.rs",
            "  read    file-20.rs",
        ]
    );
    // Then the blank row under the task's own row.
    let paned = terminal(40).pane(&board(20), 8);
    assert_eq!(
        texts(&paned.lines),
        [
            "task-1  Write the greeting",
            "  proved   red",
            "  by      test -f greeting.txt",
            "",
            "  what it is doing",
            "  ... 18 above",
            "  read    file-19.rs",
            "  read    file-20.rs",
        ]
    );
}

#[test]
fn at_the_least_terminal_what_a_region_says_at_the_least_stands_whole_and_the_feed_keeps_the_rest()
{
    // A titled pane on a screen standing short of rows stands close: the task's row alone
    // stands over the feed, whose window keeps its least.
    let titled = board(20).titled(["implement", "loop"]);
    let paned = terminal(40).pane_short_of_rows(&titled, 5);
    assert_eq!(
        texts(&paned.lines),
        [
            "task-1  Write the greeting",
            "  what it is doing",
            "  ... 18 above",
            "  read    file-19.rs",
            "  read    file-20.rs",
        ]
    );
    // Two tasks' rows take more: the feed keeps its count and its newest row.
    let two = Region::new(Doc::new().with(task()).with(task()).with(calls(20)))
        .abridged(vec![vec![rows(&["task-1", "task-2"])]])
        .titled(["implement", "loop"])
        .in_sight(Sight::at_the_end());
    let paned = terminal(40).pane_short_of_rows(&two, 5);
    assert_eq!(
        texts(&paned.lines),
        [
            "task-1",
            "task-2",
            "  what it is doing",
            "  ... 19 above",
            "  read    file-20.rs",
        ]
    );
    // Rows fewer still: the feed keeps its newest row, and what stands above gives way from
    // its top for it.
    let paned = terminal(40).pane_short_of_rows(&two, 3);
    assert_eq!(
        texts(&paned.lines),
        ["task-2", "  what it is doing", "  read    file-20.rs"]
    );
}

/// Five rows a region says at the least.
fn five() -> Block {
    rows(&["one", "two", "three", "four", "five"])
}

#[test]
fn an_aired_pane_lets_the_last_abridgement_give_way_from_its_top() {
    let region = Region::new(Doc::new().with(task()).with(five()).with(calls(20)))
        .abridged(vec![vec![five()]])
        .in_sight(Sight::at_the_end());
    let paned = terminal(40).pane(&region, 8);
    assert_eq!(
        texts(&paned.lines),
        [
            "three",
            "four",
            "five",
            "",
            "  what it is doing",
            "  ... 18 above",
            "  read    file-19.rs",
            "  read    file-20.rs",
        ]
    );
}
