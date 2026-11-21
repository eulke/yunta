//! A feed is a block every surface draws: as Markdown, as a page, and on a terminal in every
//! environment, each row one row however long it is.

use std::path::{Path, PathBuf};

use super::Feed;
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::golden::{assert_golden, ENVIRONMENTS};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/feed")
}

/// A node's feed scrolled back: eight of its calls in sight, four above them and four under.
pub(crate) fn scrolled_back() -> Feed {
    let rows = [
        "search  global pack",
        "read    crates/cli/src/pack.rs",
        "read    crates/cli/src/commands/pack.rs",
        "read    crates/testkit/src/bin.rs",
        "edit    crates/cli/src/pack.rs",
        "edit    crates/cli/src/cli/pack.rs",
        "edit    crates/cli/src/commands/pack.rs",
        "edit    crates/cli/src/cli/dispatch.rs",
    ];
    Feed {
        rows: rows.into_iter().map(|row| Line::new().plain(row)).collect(),
        subjects: Vec::new(),
        before: 4,
        after: 4,
    }
}

/// A node's feed at its end, its rows said with `glyphs`: its first attempt's check red, the
/// second attempt opened, a command longer than a narrow line, and the newest row strong.
fn at_its_end(glyphs: Glyphs) -> Feed {
    let sep = glyphs.sep();
    let rows = [
        format!("global-pack-install post: cargo test --workspace {sep} red {sep} 3 of 412 failed in 1m31s"),
        "running: attempt 2".to_string(),
        format!("session opened on gpt-6-luna {sep} fence widened to 1 root"),
        "run     cargo test --workspace --test pack_global_spec global_pack_commands_use_the_global_root"
            .to_string(),
    ];
    Feed {
        rows: rows
            .into_iter()
            .map(|row| Line::new().plain(row))
            .chain([Line::new().push(Tone::Strong, "edit    crates/cli/src/pack.rs")])
            .collect(),
        subjects: Vec::new(),
        before: 3,
        after: 0,
    }
}

/// A golden case: its name, and its feed with its rows said in the glyphs it is given.
type Case = (&'static str, fn(Glyphs) -> Feed);

#[test]
fn a_feed_matches_its_goldens_on_every_surface() {
    let cases: [Case; 2] = [
        ("scrolled-back", |_| scrolled_back()),
        ("at-its-end", at_its_end),
    ];
    for (case, feed) in cases {
        let doc = Doc::new().with(feed(Glyphs::Unicode));
        assert_golden(&goldens().join(format!("{case}.md")), &Markdown.draw(&doc));
        assert_golden(&goldens().join(format!("{case}.html")), &Html.body(&doc));
        for environment in &ENVIRONMENTS {
            let look = Look::of(environment);
            let drawn = Terminal::on(look).draw(&Doc::new().with(feed(look.glyphs)));
            assert_golden(&environment.golden(&goldens(), case), &drawn);
        }
    }
}

/// A row past the edge of a pane ends in the ellipsis at the edge and stays one row, where the
/// pane folds a line; the cut falls before a number it would split or end on, since a number
/// cut short reads as a smaller one.
#[test]
fn a_row_past_the_edge_ends_in_the_ellipsis_and_cuts_no_number() {
    let row = "failed: the suite exits 101 after 1m31s";
    for (edge, drawn) in [
        // Inside `1m31s`: the cut falls before it.
        (36, "failed: the suite exits 101 after…"),
        // Right after `101`, which is whole: the cut still falls before it.
        (28, "failed: the suite exits…"),
        // Inside `101`.
        (26, "failed: the suite exits…"),
        // Inside a word, with every number before the cut whole.
        (30, "failed: the suite exits 101 a…"),
    ] {
        let pane = Look {
            glyphs: Glyphs::Unicode,
            paint: Paint::Plain,
            width: Width::exact(edge),
            motion: Motion::Moves,
        };
        let feed = Feed {
            rows: vec![Line::new().plain(row)],
            subjects: Vec::new(),
            before: 0,
            after: 0,
        };
        let lines = Terminal::on(pane).block(&feed.into());
        let texts: Vec<String> = lines.iter().map(Line::text).collect();
        assert_eq!(texts, [drawn], "at {edge} cells");
    }
}

/// A feed of more than one thing's moments opens each row with what it is about, in a column
/// as wide as the widest subject among the rows a drawing shows and two cells: a window at the
/// feed's end measures the rows in its sight, so a subject scrolled out of sight widens it no
/// more, and a drawing of every row measures them all.
#[test]
fn each_row_opens_with_its_subject_in_a_column_as_wide_as_the_widest_in_sight() {
    let said = [
        ("approve-plan", "finished: approve · 0s"),
        ("implement", "running: attempt 1"),
        ("implement", "read    crates/cli/src/pack.rs"),
        ("implement", "run     cargo check"),
    ];
    let feed = Feed {
        rows: said
            .iter()
            .map(|(_, words)| Line::new().plain(*words))
            .collect(),
        subjects: said
            .iter()
            .map(|(subject, _)| subject.to_string())
            .collect(),
        before: 0,
        after: 0,
    };
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(60),
        motion: Motion::Still,
    };
    let region = crate::components::Region::new(Doc::new().with(feed))
        .in_sight(crate::components::Sight::at_the_end());
    let window = Terminal::on(look).pane(&region, 3);
    let texts: Vec<String> = window.lines.iter().map(Line::text).collect();
    assert_eq!(
        texts,
        [
            "… 2 above",
            "implement  read    crates/cli/src/pack.rs",
            "implement  run     cargo check",
        ]
    );
    let whole = Terminal::on(look).draw(&region.doc);
    assert!(
        whole.contains("approve-plan  finished: approve · 0s\nimplement     running: attempt 1"),
        "{whole}"
    );
}
