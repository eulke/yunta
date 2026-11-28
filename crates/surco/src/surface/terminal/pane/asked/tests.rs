//! A line or a list asked on a page of its own: a line asked alone opens on a blank row where
//! the pane has room, and on a screen standing short of rows each stands close.

use crate::components::{
    ChoiceList, ChoiceRow, Choices, Legend, Measure, Region, TypedAnswer, TypedRow,
};
use crate::doc::Doc;
use crate::golden::ENVIRONMENTS;
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::surface::Terminal;

/// The look of a pane 80 cells wide, in ASCII with no paint: a pane's width is exact.
fn pane_look() -> Look {
    Look {
        width: Width::exact(ENVIRONMENTS[1].width),
        ..Look::of(&ENVIRONMENTS[1])
    }
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
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

/// A line asked alone opens on a blank row, as a form opens, where the pane holds the line and
/// that row, the cursor counted from the pane's top; the blank row gives way first where the
/// pane holds the line alone, and on a screen standing short of rows.
#[test]
fn a_line_asked_alone_opens_on_a_blank_row_that_gives_way_first() {
    let terminal = Terminal::on(pane_look());
    let region = Region::new(Doc::new().with(named()));
    let asked = "project name (enter keeps `yunta`)".to_string();
    let [top, words, bottom] = boxed("yunta-tui", 80);
    let line = [asked, top, words, bottom];
    let cell = "| yunta-".len();
    let aired = terminal.pane(&region, 6);
    let opened: Vec<String> = std::iter::once(String::new()).chain(line.clone()).collect();
    assert_eq!(texts(&aired.lines), opened);
    assert_eq!(aired.cursor, Some((3, cell)));
    let tight = terminal.pane(&region, 4);
    assert_eq!(texts(&tight.lines), line);
    assert_eq!(tight.cursor, Some((2, cell)));
    let close = terminal.pane_short_of_rows(&region, 6);
    assert_eq!(texts(&close.lines), line);
    assert_eq!(close.cursor, Some((2, cell)));
}

/// A list picked from on a page of its own stands a blank row under what asks it; on a screen
/// standing short of rows, right under it, the list's own rows as they stand.
#[test]
fn a_list_asked_on_a_page_stands_right_under_its_question_short_of_rows() {
    let terminal = Terminal::on(pane_look());
    let row = |name: &str| ChoiceRow {
        name: name.to_string(),
        said: None,
        cost: None,
        key: None,
    };
    let list = ChoiceList {
        choices: Some(Choices {
            rows: vec![row("tasks"), row("spec")],
            at: 0,
        }),
        filter: None,
        legend: Legend::new(),
    };
    let asked = vec![Line::new().push(Tone::Strong, "Which document's shape?")];
    let region = Region::new(Doc::new().with(asked).with(list));
    let aired = texts(&terminal.pane(&region, 10).lines);
    assert_eq!(aired[..2], ["Which document's shape?", ""], "{aired:#?}");
    let close = texts(&terminal.pane_short_of_rows(&region, 10).lines);
    assert_eq!(close[0], "Which document's shape?", "{close:#?}");
    assert_eq!(close[1..], aired[2..], "the list as it stands: {close:#?}");
}

/// A line asked alone set to a measure from the left stands at the pane's left edge, its box as
/// wide as the measure, where a pane as narrow as the measure or narrower holds it across.
#[test]
fn a_line_asked_on_a_measure_from_the_left_stands_at_the_panes_edge() {
    let terminal = Terminal::on(pane_look());
    let measured = Region::new(Doc::new().with(named())).measured(Measure::of(40).from_the_left());
    let paned = terminal.pane(&measured, 6);
    let [top, words, bottom] = boxed("yunta-tui", 40);
    let asked = "project name (enter keeps `yunta`)".to_string();
    assert_eq!(
        texts(&paned.lines),
        ["".to_string(), asked, top, words, bottom]
    );
    assert_eq!(paned.cursor, Some((3, "| yunta-".len())));
    let across = Region::new(Doc::new().with(named())).measured(Measure::of(80).from_the_left());
    let [top, ..] = boxed("yunta-tui", 80);
    assert_eq!(texts(&terminal.pane(&across, 6).lines)[2], top);
}
