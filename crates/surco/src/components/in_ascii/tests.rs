use super::*;
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::line::Line;
use crate::look::{Look, Width};
use crate::motion::Motion;
use crate::paint::Paint;
use crate::surface::{Markdown, Surface, Terminal};
use crate::width::cell_width;

/// What a session said when its credential was refused, a separator of its own among it.
const REFUSED: &str = "node `grill` failed: session `mock` refused the request (its credential \
                       was refused): Not logged in · Please run /login.";

/// Words with letters that carry marks and the dash a pack writes.
const MARKED: &str = "diseño — más";

/// Characters with no form: a wide one, an emoji, and a letter with a combining mark.
const NO_FORM: &str = "漢 🚀 e\u{301}";

/// An unpainted terminal in the ASCII glyphs, its lines exactly `cells` wide.
fn ascii(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Ascii,
        paint: Paint::Plain,
        width: Width::exact(cells),
        motion: Motion::Still,
    }
}

/// A screen whose main region says each of `said` as prose, under a trail that names a
/// place with a marked letter.
fn screen_saying(said: &[&str]) -> Screen {
    let doc = said
        .iter()
        .fold(Doc::new(), |doc, said| doc.with(Prose(said.to_string())));
    Screen::new(Region::new(doc).titled(["plan"])).trail(["yunta", "/cut/diseño"])
}

/// `lines` read as one text: each trimmed, a space between two.
fn read(lines: &[Line]) -> String {
    let texts: Vec<String> = lines
        .iter()
        .map(|line| line.text().trim().to_string())
        .collect();
    texts.join(" ")
}

#[test]
fn a_screens_line_drawn_in_the_ascii_glyphs_holds_ascii_alone() {
    let screen = screen_saying(&[REFUSED, MARKED, NO_FORM]).in_ascii();
    let terminal = Terminal::on(ascii(118));
    let lines = terminal.pane(&screen.main, 20).lines;
    let trail = terminal.trail(&screen.trail, &screen.tally);
    let drawn: Vec<String> = lines.iter().chain([&trail]).map(Line::text).collect();
    assert!(drawn.iter().all(|row| row.is_ascii()), "{drawn:#?}");
    let said = read(&lines);
    assert!(
        said.contains("Not logged in ; Please run /login."),
        "{said}"
    );
    assert!(said.contains("diseno -- mas"), "{said}");
    assert!(said.contains("? ? e?"), "{said}");
    assert_eq!(trail.text(), "yunta :: /cut/diseno");
}

/// A row is cut by what it draws: a row as wide as its pane, which its dash makes a cell wider
/// in ASCII, is cut inside the pane, where a fold after the cut would pass its edge.
#[test]
fn a_screens_row_is_cut_by_the_ascii_it_draws() {
    let row = "twenty cells — wide.";
    assert_eq!(cell_width(row), 20);
    let region = Region::new(Doc::new().with(Block::Rows(vec![Line::new().plain(row)])));
    let screen = Screen::new(region).in_ascii();
    let drawn = Terminal::on(ascii(20)).pane(&screen.main, 4).lines;
    let first = drawn.first().map(Line::text).unwrap_or_default();
    assert!(
        cell_width(&first) <= 20 && first.starts_with("twenty cells --") && first.ends_with("..."),
        "{first}"
    );
}

/// The same words printed as a page in the ASCII glyphs keep every character, and so does
/// Markdown: a page never loses a character of what was recorded.
#[test]
fn the_same_line_printed_as_a_page_keeps_its_characters() {
    let page = Doc::new()
        .with(Prose(REFUSED.to_string()))
        .with(Prose(MARKED.to_string()))
        .with(Prose(NO_FORM.to_string()));
    let printed = Terminal::on(Look::plain()).lines(&page);
    let said = read(&printed);
    assert!(
        said.contains("Not logged in · Please run /login."),
        "{said}"
    );
    assert!(said.contains(MARKED), "{said}");
    assert!(said.contains(NO_FORM), "{said}");
    let markdown = Markdown.screen(&screen_saying(&[REFUSED, MARKED]));
    assert!(markdown.contains("Not logged in · Please") && markdown.contains(MARKED));
}

/// A cell's id stands on the same words once the text before it took a longer form.
#[test]
fn a_cells_id_is_marked_again_where_its_text_moved() {
    let text = "ñ… a1B2";
    let start = text.find("a1B2").unwrap_or_default();
    let cell = Cell {
        text: text.to_string(),
        tone: Tone::Plain,
        id: Some(start..text.len()),
    }
    .in_ascii();
    assert_eq!(cell.text, "n... a1B2");
    assert_eq!(cell.id.map(|id| &cell.text[id]), Some("a1B2"));
}

/// A quoted line whose fault stands after a character that takes a longer form in ASCII keeps
/// its marks under the fault once folded, and the place it names stays as recorded, the
/// column an editor opens.
#[test]
fn a_fault_folded_keeps_its_marks_under_it_and_its_place_as_recorded() {
    let place = Place {
        path: "w.yaml".to_string(),
        line: 3,
        col: 8,
        source: Some("a — b: bad".to_string()),
        len: 3,
        marked: None,
    };
    let problem = Problem::error("a value it does not read", Some(place));
    let drawn = Terminal::on(ascii(60)).draw(&Doc::new().with(problem.in_ascii()));
    let rows: Vec<&str> = drawn.lines().collect();
    assert!(
        drawn.contains("w.yaml:3:8"),
        "the place as recorded:\n{drawn}"
    );
    let quoted = rows.iter().find(|row| row.contains("a -- b: bad"));
    let quoted = quoted.expect("the quoted line, folded");
    let marks = rows.iter().find(|row| row.trim_end().ends_with("^^^"));
    let marks = marks.expect("the marks under its fault");
    assert_eq!(marks.find('^'), quoted.find("bad"), "\n{drawn}");
}

/// The window's title takes the screen's fold: in ASCII where the glyphs are, so a terminal
/// that names no UTF-8 locale draws it as it draws the screen's rows; as written in Unicode.
#[test]
fn the_windows_title_takes_the_screens_fold() {
    let mut screen = screen_saying(&[]);
    screen.title = Some(format!("Q78GRR needs you: {MARKED}"));
    let titled = Terminal::on(ascii(80)).window_title(&screen);
    let titled = titled.expect("a title");
    assert!(titled.is_ascii(), "{titled}");
    assert_eq!(titled, screen.in_ascii().title.expect("a title"));
    let unicode = Look {
        glyphs: Glyphs::Unicode,
        ..ascii(80)
    };
    assert_eq!(Terminal::on(unicode).window_title(&screen), screen.title);
}

/// Every block of the catalog, and one more of each kind whose text is taken apart by hand
/// (a cell marked by its id, a place in a quoted line, a section's title, a run's edge, a
/// card's words), each saying words outside ASCII, held in a screen folded to ASCII: every row
/// a terminal in the ASCII glyphs draws of it holds ASCII alone, at every width it is drawn.
#[test]
fn every_block_of_the_catalog_folded_draws_ascii_alone() {
    let seeded = Doc::new()
        .with(Table {
            columns: vec![Column {
                title: "task",
                holds: Holds::Id,
            }],
            rows: vec![Row {
                mark: None,
                cells: vec![Cell {
                    text: format!("{MARKED} a1B2"),
                    tone: Tone::Plain,
                    id: Some(MARKED.len() + 1..MARKED.len() + 5),
                }],
            }],
        })
        .with(Problem::error(
            MARKED,
            Some(Place {
                path: format!("{MARKED}.yaml"),
                line: 3,
                col: 2,
                source: Some(format!("a {MARKED}")),
                len: 3,
                marked: None,
            }),
        ))
        .with(Section {
            lead: None,
            title: Line::new().plain(MARKED),
            blocks: vec![Prose(NO_FORM.to_string()).into()],
        })
        .with(Note(REFUSED.to_string()));
    let doc = crate::components::catalog::catalog().with_all(seeded.into_blocks());
    let screen = Screen::new(Region::new(doc)).in_ascii();
    for cells in [58, 78, 118] {
        let terminal = Terminal::on(ascii(cells));
        let drawn = terminal.lines(&screen.main.doc);
        let outside: Vec<String> = drawn
            .iter()
            .map(Line::text)
            .filter(|row| !row.is_ascii())
            .collect();
        assert!(outside.is_empty(), "{cells} cells: {outside:#?}");
    }
}

/// A word named in Yunta's source is written in ASCII: one outside it fails a test build that
/// folds it, as no fold can give it a form that lives as long as the program.
#[test]
#[should_panic(expected = "written in ASCII")]
fn a_word_named_in_the_source_outside_ascii_fails_its_fold() {
    let fields = Fields::new().push_if("diseño", "a value");
    let _ = Block::from(fields).in_ascii();
}
