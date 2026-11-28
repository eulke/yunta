//! A region in a pane: a prompt held in the rows what stands around it leaves, with the
//! cursor counted from the pane's top, what stands above it giving way from its top and
//! what stands under it kept whole where the prompt fits beside it; a feed at a region's
//! foot in its window, under what stands above it, which gives way from its top; any other
//! region scrolled, as a document scrolls. A titled region opens a row under its title, and
//! a pane in its fewest rows stands close.

use std::path::{Path, PathBuf};

use crate::components::{
    Card, CardBody, Choices, Desk, Feed, FeedRow, Laid, Note, Prose, Region, Sight, TypedAnswer,
    TypedRow,
};
use crate::doc::{Block, Doc};
use crate::glyphs::{Glyphs, Mark};
use crate::golden::{assert_golden, Environment, ENVIRONMENTS};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::surface::terminal::card::tests::{card, eight_rows};
use crate::surface::terminal::desk::tests::{desk, numbered};
use crate::surface::Terminal;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/pane")
}

/// The look of a pane as wide as `environment`'s lines: a pane's width is exact.
fn pane_look(environment: &Environment) -> Look {
    Look {
        width: Width::exact(environment.width),
        ..Look::of(environment)
    }
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
}

/// A page of `count` lines, `line 1` to `line <count>`.
fn page(count: usize) -> Doc {
    let lines: Vec<Line> = (1..=count)
        .map(|at| Line::new().plain(format!("line {at}")))
        .collect();
    Doc::new().with(lines)
}

/// A project's name asked, its detected value standing, and a name half typed over it.
fn named() -> TypedAnswer {
    TypedAnswer {
        asked: Line::new()
            .push(Tone::Strong, "project name")
            .push(Tone::Muted, " (enter keeps `yunta`)"),
        typed: TypedRow {
            prompt: "> ".to_string(),
            before: "yunta-".to_string(),
            after: "tui".to_string(),
        },
        placeholder: None,
        hint: None,
    }
}

/// A decision of eight options, the third pointed at.
fn choosing(glyphs: Glyphs) -> Card {
    let body = CardBody::Choosing {
        choices: Choices {
            rows: eight_rows(),
            at: 2,
        },
        then: None,
    };
    card(body, glyphs)
}

/// A plan's desk of thirty lines to read.
fn plan() -> Desk {
    desk(numbered(30))
}

/// What a project is called, asked under the sentence that says what the name is for.
fn named_under_a_sentence() -> Region {
    Region::new(
        Doc::new()
            .with(Prose("The name the project goes by.".to_string()))
            .with(named()),
    )
}

/// The row a person types on, boxed `cells` wide in ASCII: the top edge, the words one
/// cell inside the uprights, the bottom edge.
fn boxed(words: &str, cells: usize) -> [String; 3] {
    let level = "-".repeat(cells - 2);
    let inside = cells - 4;
    [
        format!(".{level}."),
        format!("| {words:<inside$} |"),
        format!("'{level}'"),
    ]
}

/// A typed answer under a paragraph: the paragraph, the question, then the box the words
/// are typed in, with the cursor inside the box on its middle row, after the words before
/// it, counted from the pane's top.
#[test]
fn a_prompt_is_held_under_what_stands_above_it_with_the_cursor_counted_from_the_top() {
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[1])).pane(&named_under_a_sentence(), 6);
    let [top, words, bottom] = boxed("yunta-tui", 80);
    assert_eq!(
        texts(&paned.lines),
        [
            "  The name the project goes by.".to_string(),
            String::new(),
            "project name (enter keeps `yunta`)".to_string(),
            top,
            words,
            bottom,
        ]
    );
    assert_eq!(paned.cursor, Some((4, "| yunta-".len())));
}

/// What stands under a prompt, as a note of what was refused, stays whole under it.
#[test]
fn what_stands_under_a_prompt_stays_whole_under_it() {
    let region = Region::new(
        Doc::new()
            .with(named())
            .with(Note("a project is already called that".to_string())),
    );
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[1])).pane(&region, 6);
    let [top, words, bottom] = boxed("yunta-tui", 80);
    assert_eq!(
        texts(&paned.lines),
        [
            "project name (enter keeps `yunta`)".to_string(),
            top,
            words,
            bottom,
            String::new(),
            "  a project is already called that".to_string(),
        ]
    );
    assert_eq!(paned.cursor, Some((2, "| yunta-".len())));
}

/// Short of the rows a prompt is asked in, what stands under it gives way from its end: the
/// question, and the box a person types in with the cursor in it, stay in sight.
#[test]
fn what_stands_under_a_prompt_short_of_rows_gives_way_from_its_end() {
    let region = Region::new(Doc::new().with(named()).with_all(page(3).into_blocks()));
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[1])).pane(&region, 7);
    let [top, words, bottom] = boxed("yunta-tui", 80);
    assert_eq!(
        texts(&paned.lines),
        [
            "project name (enter keeps `yunta`)".to_string(),
            top,
            words,
            bottom,
            String::new(),
            "line 1".to_string(),
            "line 2".to_string(),
        ]
    );
    assert_eq!(paned.cursor, Some((2, "| yunta-".len())));
}

/// Short of rows, what stands above a prompt other than a card keeps at most half the pane,
/// from the line nearest the prompt up: the prompt is what a person answers, and its question
/// stays. A card stands at the foot by a rule of its own (`card_at_the_foot`).
#[test]
fn what_stands_above_a_prompt_gives_way_from_its_top() {
    let region = Region::new(Doc::new().with_all(page(10).into_blocks()).with(named()));
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[1])).pane(&region, 10);
    let text = texts(&paned.lines);
    assert!(paned.lines.len() <= 10, "{text:#?}");
    assert_eq!(text[0], "line 7", "{text:#?}");
    assert_eq!(text[3], "line 10", "{text:#?}");
    assert!(
        text.iter().any(|line| line.contains("project name")),
        "{text:#?}"
    );
}

/// A pane is no terminal's bottom: a prompt in one fills every row it is given, where
/// held where a person is it keeps the terminal's last row free.
#[test]
fn a_prompt_in_a_pane_fills_every_row_it_is_given() {
    let terminal = Terminal::on(pane_look(&ENVIRONMENTS[1]));
    let paned = terminal.pane(&Region::new(Doc::new().with(plan())), 12);
    assert_eq!(paned.lines.len(), 12, "{:#?}", texts(&paned.lines));
    assert!(
        paned.laid.page > 0,
        "what scrolls in the desk is handed back"
    );
    let held = terminal.held(&Block::from(plan()), 80, 12);
    assert_eq!(held.lines.len(), 11);
}

/// A region with no prompt scrolls from its sight, as a document scrolls.
#[test]
fn a_region_with_no_prompt_scrolls_from_its_sight() {
    let terminal = Terminal::on(pane_look(&ENVIRONMENTS[1]));
    let region = Region::new(page(20)).in_sight(Sight::from_line(3));
    let paned = terminal.pane(&region, 5);
    let (lines, laid) = terminal.scrolled(&page(20), Sight::from_line(3), 5);
    assert_eq!(texts(&paned.lines), texts(&lines));
    assert_eq!(paned.laid, laid);
    assert_eq!(paned.cursor, None);
}

/// What a node at work hands over, under its heading, then what it is doing under its own,
/// in `glyphs`: three blocks above its feed.
fn handing_over(glyphs: Glyphs) -> Doc {
    let sep = glyphs.sep();
    let not_yet = format!("{} tasks  not yet", glyphs.mark(Mark::Absent));
    Doc::new()
        .with(Block::Heading("what it hands over".to_string()))
        .with(Block::Rows(vec![Line::new().plain(not_yet)]))
        .with(Block::Heading(format!("what it is doing {sep} 3 calls")))
}

/// A node's first five moments as its feed lists them, in `glyphs`, the newest strong: the
/// whole feed, at its end.
fn five_moments(glyphs: Glyphs) -> Feed {
    let sep = glyphs.sep();
    let rows = [
        "running: attempt 1".to_string(),
        format!("session opened on gpt-6-luna {sep} fence widened to 1 root"),
        "read    crates/cli/src/commands/pack.rs".to_string(),
        "read    crates/cli/src/cli/pack.rs".to_string(),
    ];
    Feed {
        rows: rows
            .into_iter()
            .map(|row| Line::new().plain(row))
            .chain([Line::new().push(Tone::Strong, "read    crates/engine/src/catalog.rs")])
            .map(FeedRow::of)
            .collect(),
        before: 0,
        after: 0,
    }
}

/// What a loop's task at work says above its feed, in `glyphs`: the task, then how it is
/// proved, then the feed's heading.
fn proving(glyphs: Glyphs) -> Doc {
    let sep = glyphs.sep();
    let task =
        format!("global-pack-install  Add explicit global pack install target {sep} attempt 1");
    let proof = [
        "proved   red",
        "by      cargo test --workspace --test pack_global_spec global_pack_commands_use_the_global_root",
        "keeps   lint, green at the baseline",
    ];
    Doc::new()
        .with(Block::Rows(vec![Line::new().plain(task)]))
        .with(Block::Rows(
            proof
                .into_iter()
                .map(|row| Line::new().plain(row))
                .collect(),
        ))
        .with(Block::Heading(format!("what it is doing {sep} 12 calls")))
}

/// A task's sixteen moments as its feed lists them, in `glyphs`, the newest strong.
fn sixteen_moments(glyphs: Glyphs) -> Feed {
    let sep = glyphs.sep();
    let rows = [
        "running: attempt 1".to_string(),
        format!("session opened on gpt-6-luna {sep} fence widened to 1 root"),
        "search  global pack".to_string(),
        "read    crates/cli/src/pack.rs".to_string(),
        "read    crates/cli/src/commands/pack.rs".to_string(),
        "read    crates/testkit/src/bin.rs".to_string(),
        "read    crates/cli/src/cli/pack.rs".to_string(),
        "read    crates/cli/src/cli/dispatch.rs".to_string(),
        "edit    crates/cli/src/pack.rs".to_string(),
        "edit    crates/cli/src/cli/pack.rs".to_string(),
        "edit    crates/cli/src/commands/pack.rs".to_string(),
        "edit    crates/cli/src/cli/dispatch.rs".to_string(),
        "run     cargo test --workspace --test pack_global_spec".to_string(),
        "edit    crates/cli/tests/pack_cmd.rs".to_string(),
        "run     cargo test --workspace --test pack_cmd".to_string(),
    ];
    Feed {
        rows: rows
            .into_iter()
            .map(|row| Line::new().plain(row))
            .chain([Line::new().push(Tone::Strong, "edit    crates/cli/src/commands/pack.rs")])
            .map(FeedRow::of)
            .collect(),
        before: 0,
        after: 0,
    }
}

/// `doc` with `feed` at its foot.
fn ending_with(doc: Doc, feed: Feed) -> Doc {
    doc.with(feed)
}

/// Three blocks over a feed of five moments, at its end, in a pane of five rows, too few for
/// all: the blocks give way from their top, a blank row left at the pane's top with them,
/// and the feed's window takes the rows they leave, its newest row the pane's last. What was
/// laid out is counted from the feed's first row: two rows stand above the three in sight.
#[test]
fn a_feed_under_blocks_in_too_few_rows_keeps_its_newest_row_last_and_cuts_the_blocks_from_their_top(
) {
    let glyphs = Glyphs::Unicode;
    let region = Region::new(ending_with(handing_over(glyphs), five_moments(glyphs)))
        .in_sight(Sight::at_the_end());
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[2])).pane(&region, 5);
    assert_eq!(
        texts(&paned.lines),
        [
            "what it is doing · 3 calls",
            "… 2 above",
            "read    crates/cli/src/commands/pack.rs",
            "read    crates/cli/src/cli/pack.rs",
            "read    crates/engine/src/catalog.rs",
        ]
    );
    assert_eq!(paned.cursor, None);
    assert_eq!(
        paned.laid,
        Laid {
            top: 2,
            page: 3,
            most: 2
        }
    );
}

/// With rows for all, the blocks above a feed stand whole and the feed fills down from
/// under its heading, the rows under it left blank.
#[test]
fn a_feed_with_rows_for_all_stands_whole_under_its_blocks() {
    let glyphs = Glyphs::Unicode;
    let region = Region::new(ending_with(handing_over(glyphs), five_moments(glyphs)))
        .in_sight(Sight::at_the_end());
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[2])).pane(&region, 12);
    let text = texts(&paned.lines);
    assert_eq!(text[0], "what it hands over", "{text:#?}");
    assert_eq!(text[4], "running: attempt 1", "{text:#?}");
    assert_eq!(text.len(), 9, "{text:#?}");
    assert!(!paned.laid.scrolls(), "{:?}", paned.laid);
}

/// A feed scrolled back to a row shows its window from that row, the rows above and under
/// it counted; what was laid out says where it stands among the feed's rows, so a key moves
/// on from there.
#[test]
fn a_feed_scrolled_back_counts_what_stands_above_and_under_its_window() {
    let glyphs = Glyphs::Unicode;
    let region = Region::new(ending_with(proving(glyphs), sixteen_moments(glyphs)))
        .in_sight(Sight::from_line(8));
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[2])).pane(&region, 13);
    let text = texts(&paned.lines);
    assert_eq!(
        text[6..],
        [
            "what it is doing · 12 calls",
            "… 8 above",
            "edit    crates/cli/src/pack.rs",
            "edit    crates/cli/src/cli/pack.rs",
            "edit    crates/cli/src/commands/pack.rs",
            "edit    crates/cli/src/cli/dispatch.rs",
            "… 4 under",
        ],
        "{text:#?}"
    );
    assert_eq!(
        paned.laid,
        Laid {
            top: 8,
            page: 5,
            most: 11
        }
    );
}

/// A feed handed as a window of a longer one, its rows above and under it counted, is laid
/// out among the whole feed's rows: its place and what was laid out are counted from the
/// whole feed's first row, so a place stays on the rows in sight whatever grows above or
/// under them.
#[test]
fn a_feeds_window_is_counted_from_the_whole_feeds_first_row() {
    let rows: Vec<Line> = (100..140)
        .map(|at| Line::new().plain(format!("read    file-{at}.rs")))
        .collect();
    let window = Feed {
        rows: rows.into_iter().map(FeedRow::of).collect(),
        before: 100,
        after: 9_860,
    };
    let region = Region::new(Doc::new().with(window)).in_sight(Sight::from_line(108));
    let paned = Terminal::on(pane_look(&ENVIRONMENTS[1])).pane(&region, 6);
    assert_eq!(
        texts(&paned.lines),
        [
            "... 108 above",
            "read    file-108.rs",
            "read    file-109.rs",
            "read    file-110.rs",
            "read    file-111.rs",
            "... 9888 under",
        ]
    );
    assert_eq!(
        paned.laid,
        Laid {
            top: 108,
            page: 5,
            most: 9_995
        }
    );
}

/// A titled pane opens a row under its title and parts two blocks by a blank row, its feed
/// at its foot or none; on a screen standing short of rows, as a node's pane at the least
/// terminal, it stands close, with neither, however many rows it has.
#[test]
fn a_titled_pane_opens_a_row_under_its_title_and_stands_close_short_of_rows() {
    let terminal = Terminal::on(pane_look(&ENVIRONMENTS[1]));
    let drawn = |fed: bool, short_of_rows: bool, rows| {
        let runs = Doc::new()
            .with(Block::Heading("what it runs".to_string()))
            .with(Block::Rows(vec![Line::new().plain("true")]));
        let running = vec![Line::new().plain("running")];
        let doc = match fed {
            true => runs.with(Block::Heading("calls".to_string())).with(Feed {
                rows: running.into_iter().map(FeedRow::of).collect(),
                before: 0,
                after: 0,
            }),
            false => runs,
        };
        let region = Region::new(doc)
            .titled(["build", "bash"])
            .in_sight(Sight::at_the_end());
        let paned = terminal.pane_holding(&region, &[], rows, short_of_rows);
        texts(&paned.lines).join("|")
    };
    assert_eq!(drawn(true, false, 8), "|what it runs|true||calls|running");
    assert_eq!(drawn(true, true, 5), "what it runs|true|calls|running");
    assert_eq!(drawn(true, true, 8), "what it runs|true|calls|running");
    assert_eq!(drawn(false, false, 8), "|what it runs|true");
    assert_eq!(drawn(false, true, 5), "what it runs|true");
}

/// A card under a sentence held in a short pane, a plan's desk in a pane of its own, a page
/// scrolled past its first lines, a feed at its end under blocks too many for the pane's rows,
/// and a feed scrolled back, each with the rows its pane has.
fn cases(glyphs: Glyphs) -> [(&'static str, Region, usize); 5] {
    let card = Doc::new()
        .with(Prose("The plan the planner handed over.".to_string()))
        .with(choosing(glyphs));
    [
        ("held-card", Region::new(card), 10),
        ("held-desk", Region::new(Doc::new().with(plan())), 14),
        (
            "scrolled-page",
            Region::new(page(30)).in_sight(Sight::from_line(5)),
            8,
        ),
        (
            "feed-at-its-end",
            Region::new(ending_with(handing_over(glyphs), five_moments(glyphs)))
                .in_sight(Sight::at_the_end()),
            5,
        ),
        (
            "feed-scrolled-back",
            Region::new(ending_with(proving(glyphs), sixteen_moments(glyphs)))
                .in_sight(Sight::from_line(8)),
            13,
        ),
    ]
}

#[test]
fn a_pane_matches_its_goldens_in_every_environment() {
    for environment in &ENVIRONMENTS {
        let look = pane_look(environment);
        let terminal = Terminal::on(look);
        for (case, region, rows) in cases(look.glyphs) {
            let paned = terminal.pane(&region, rows);
            let drawn: String = paned
                .lines
                .iter()
                .map(|line| format!("{}\n", terminal.look.paint.line(line)))
                .collect();
            assert_golden(&environment.golden(&goldens(), case), &drawn);
        }
    }
}
