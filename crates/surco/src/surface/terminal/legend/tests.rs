use super::*;
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::look::Width;
use crate::paint::Paint;
use crate::surface::{Html, Markdown, Surface, Terminal};

/// A plain look `cells` wide, in Unicode.
fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::of(Some(cells), None),
        motion: crate::motion::Motion::Moves,
    }
}

/// A legend with no key has nothing to say on any surface.
#[test]
fn an_empty_legend_draws_nothing() {
    let doc = || Doc::new().with(Legend::new());
    assert!(Legend::new().lines(&look(80)).is_empty());
    assert_eq!(Terminal::on(look(80)).draw(&doc()).trim(), "");
    assert_eq!(Markdown.draw(&doc()).trim(), "");
    assert!(!Html.body(&doc()).contains("legend"));
}

/// A legend wider than its line goes on the next one between two keys, never
/// between a key and what it does.
#[test]
fn a_narrow_legend_goes_on_between_two_keys() {
    let legend = Legend::new()
        .arrows("move to another node")
        .key("enter", "open the one pointed at")
        .key("s", "show what it cost")
        .key("q", "leave, and the run keeps going");

    let lines: Vec<String> = legend.lines(&look(60)).iter().map(Line::text).collect();

    assert_eq!(
        lines,
        [
            "↑↓ move to another node · enter open the one pointed at",
            "s show what it cost · q leave, and the run keeps going",
        ]
    );
}

/// On a screen, the legend is one row: every key where they all fit; where they do not,
/// the first keys that fit, a mark that more stand between, and the last key.
#[test]
fn the_legend_row_keeps_its_first_keys_and_its_last() {
    let legend = Legend::new()
        .arrows("node")
        .key("s", "stats")
        .key("g", "graph")
        .key("?", "the keys here");
    let wide = Terminal::on(Look {
        width: Width::exact(78),
        ..look(78)
    });
    assert_eq!(
        wide.legend(&legend, &Legend::new()).text(),
        "↑↓ node · s stats · g graph · ? the keys here"
    );
    let narrow = Terminal::on(Look {
        width: Width::exact(40),
        ..look(40)
    });
    assert_eq!(
        narrow.legend(&legend, &Legend::new()).text(),
        "↑↓ node · s stats · … · ? the keys here"
    );
}

/// A last key too wide to stand beside even the first leaves the first keys that fit,
/// cut.
#[test]
fn a_last_key_too_wide_leaves_the_first_keys_cut() {
    let legend = Legend::new()
        .key("q", "field, it keeps going")
        .key("s", "stats");
    let narrow = Terminal::on(Look {
        width: Width::exact(12),
        ..look(12)
    });
    assert_eq!(
        narrow.legend(&legend, &Legend::new()).text(),
        "q field, it…"
    );
}

/// A legend of one key wider than its row is cut at the row's width, as a row of first keys
/// is: the row a screen keeps for its keys is one row.
#[test]
fn one_key_wider_than_its_row_is_cut() {
    let legend = Legend::new().key("q", "leave, and the run keeps going");
    let narrow = Terminal::on(Look {
        width: Width::exact(12),
        ..look(12)
    });
    assert_eq!(
        narrow.legend(&legend, &Legend::new()).text(),
        "q leave, an…"
    );
}

/// The keys a person presses anywhere they are not typing: the palette's, then help's, which
/// lists every key.
fn anywhere() -> Legend {
    Legend::new().key("/", "anything").key("?", "keys")
}

/// The keys of a run's screen while it works.
fn at_work() -> Legend {
    Legend::new()
        .key("j k", "node")
        .key("s", "stats")
        .key("g", "graph")
        .key("q", "field, it keeps going")
}

/// A row exactly `cells` wide, in `glyphs`.
fn row(cells: usize, glyphs: Glyphs) -> Terminal {
    Terminal::on(Look {
        glyphs,
        width: Width::exact(cells),
        ..look(cells)
    })
}

/// On a terminal of 120 columns, inside its margins, the far keys end the row and the
/// screen's keys start it; at 80 and 60 the far keys are left out but help's, which follows
/// the screen's keys and stays last where the row cuts them.
#[test]
fn the_far_keys_end_a_wide_row_and_leave_their_last_to_a_narrow_one() {
    let wide = row(118, Glyphs::Unicode).legend(&at_work(), &anywhere());
    assert_eq!(
        wide.text(),
        format!(
            "j k node · s stats · g graph · q field, it keeps going{}/ anything · ? keys",
            " ".repeat(45)
        )
    );
    let at_80 = row(78, Glyphs::Unicode).legend(&at_work(), &anywhere());
    assert_eq!(
        at_80.text(),
        "j k node · s stats · g graph · q field, it keeps going · ? keys"
    );
    let at_60 = row(58, Glyphs::Unicode).legend(&at_work(), &anywhere());
    assert_eq!(at_60.text(), "j k node · s stats · g graph · … · ? keys");
}

/// The far keys stand from a row of 98 cells, a terminal of 100 columns inside its margins,
/// and not on a row one cell narrower.
#[test]
fn the_far_keys_stand_from_a_row_of_98_cells() {
    let at = |cells| {
        row(cells, Glyphs::Unicode)
            .legend(&at_work(), &anywhere())
            .text()
    };
    assert!(at(98).ends_with("  / anything · ? keys"), "{}", at(98));
    assert_eq!(
        at(97),
        "j k node · s stats · g graph · q field, it keeps going · ? keys"
    );
}

/// Where the screen's keys do not all fit beside the far keys, the first that fit stand with a
/// mark that more stand after them, and the far keys keep their place: ASCII's arrows take five
/// cells more than Unicode's.
#[test]
fn keys_too_many_for_the_far_keys_end_in_a_mark_and_leave_them_their_place() {
    let deciding = Legend::new()
        .key("1 2 3", "answer")
        .key("tab", "next place")
        .arrows("scroll")
        .key("p", "proof")
        .key("d", "decided for you")
        .key("esc", "leave it waiting");
    let ascii = row(118, Glyphs::Ascii).legend(&deciding, &anywhere());
    assert_eq!(
        ascii.text(),
        format!(
            "1 2 3 answer ; tab next place ; up/down scroll ; p proof ; d decided for you ; ...{}/ anything ; ? keys",
            " ".repeat(17)
        )
    );
}

/// With no far keys, as where a line is typed, a wide row holds the screen's keys alone.
#[test]
fn with_no_far_keys_a_wide_row_holds_the_screens_keys_alone() {
    let typed = Legend::new()
        .key("enter", "record it")
        .key("esc", "go back");
    let wide = row(118, Glyphs::Unicode).legend(&typed, &Legend::new());
    assert_eq!(wide.text(), "enter record it · esc go back");
}

/// Where a line is typed no key lists every key: a row too narrow for the screen's keys keeps
/// the first that fit and a mark that more stand after them, and keeps no last key.
#[test]
fn a_typed_row_too_narrow_keeps_its_first_keys_and_a_mark() {
    let round = Legend::new()
        .arrows("choose")
        .key("enter", "answer")
        .key("shift-tab", "go back")
        .key("esc", "leave it for later");
    let at_60 = row(58, Glyphs::Unicode).typed_legend(&round);
    assert_eq!(
        at_60.text(),
        "↑↓ choose · enter answer · shift-tab go back · …"
    );
    let at_80 = row(78, Glyphs::Unicode).typed_legend(&round);
    assert_eq!(
        at_80.text(),
        "↑↓ choose · enter answer · shift-tab go back · esc leave it for later"
    );
}
