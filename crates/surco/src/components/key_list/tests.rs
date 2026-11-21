//! A list of keys a terminal draws in its column, held in the rows a box gives it: the blank
//! rows give way first, then the keys' window, counting what it hides.

use super::KeyList;
use crate::components::{Legend, Sight};
use crate::doc::Doc;
use crate::golden::ENVIRONMENTS;
use crate::line::Line;
use crate::look::{Look, Width};
use crate::surface::Terminal;

/// What help lists over a run at work: its sentence, then the keys every screen answers to
/// and the run's, `pgup pgdn` the widest.
pub(crate) fn help() -> KeyList {
    KeyList {
        said: Some("You never have to remember more than this.".to_string()),
        keys: Legend::new()
            .key(
                "/",
                "anything: only what can be done here, with its command",
            )
            .key("n", "start a new run, wherever no prompt stands")
            .key("j k", "move; the footer always says what they move")
            .key("pgup pgdn", "scroll the pane a page at a time")
            .key("enter", "open, answer, confirm")
            .key("esc", "one step back; what it leaves stays waiting")
            .key("a", "go to what needs you")
            .key("f", "follow what the run is doing")
            .key("s g", "what it cost, its graph")
            .key("c", "stop it and everything under it")
            .key("o", "the whole page, in a browser")
            .key("q", "go to the field; it keeps going")
            .key("ctrl-c", "leave: every run keeps going without this window"),
    }
}

/// `list` held in a region `cells` wide and `rows` high, from the key `top`.
fn held(list: KeyList, cells: usize, rows: usize, top: usize) -> Vec<String> {
    let terminal = Terminal::on(Look {
        width: Width::exact(cells),
        ..Look::of(&ENVIRONMENTS[2])
    });
    let paned = terminal.pane(
        &crate::components::Region::new(Doc::new().with(list)).in_sight(Sight::from_line(top)),
        rows,
    );
    paned.lines.iter().map(Line::text).collect()
}

/// The keys stand in a column of seven cells, `enter` and two; a key wider than that leaves
/// widens it to the key and two cells.
#[test]
fn a_key_wider_than_the_column_widens_it_to_the_key_and_two_cells() {
    let narrow = KeyList {
        said: None,
        keys: Legend::new()
            .key("enter", "open, answer, confirm")
            .key("ctrl-c", "leave"),
    };
    assert_eq!(
        held(narrow, 60, 10, 0)[..2],
        ["enter  open, answer, confirm", "ctrl-c leave"]
    );
    let rows = held(help(), 67, 20, 0);
    assert_eq!(
        rows[2],
        "/          anything: only what can be done here, with its command"
    );
    assert_eq!(rows[5], "pgup pgdn  scroll the pane a page at a time");
}

/// Held in every row it needs, the list keeps a blank row under its sentence and two at its
/// foot; short of rows, those at its foot go first, then the one under the sentence.
#[test]
fn short_of_rows_the_blank_rows_give_way_first() {
    let whole = held(help(), 67, 18, 0);
    assert_eq!(whole.len(), 17, "{whole:#?}");
    assert_eq!(whole[1], "");
    assert_eq!(whole[15..], ["", ""]);
    let one_blank = held(help(), 67, 15, 0);
    assert_eq!(one_blank.len(), 15, "{one_blank:#?}");
    assert_eq!(one_blank[1], "");
    assert!(one_blank[14].starts_with("ctrl-c"), "{one_blank:#?}");
    let none = held(help(), 67, 14, 0);
    assert_eq!(
        none[1],
        "/          anything: only what can be done here, with its command"
    );
    assert!(none[13].starts_with("ctrl-c"), "{none:#?}");
}

/// Short of rows for every key, the sentence stays and the keys' window counts what it
/// hides above and under it, where the keys stand.
#[test]
fn the_keys_window_counts_what_it_hides() {
    let first = held(help(), 53, 7, 0);
    assert_eq!(first[0], "You never have to remember more than this.");
    assert_eq!(
        first[1],
        "/          anything: only what can be done here, wit…"
    );
    assert_eq!(first[6], "… 8 under");
    let scrolled = held(help(), 53, 7, 4);
    assert_eq!(scrolled[1], "… 4 above");
    assert_eq!(scrolled[2], "enter      open, answer, confirm");
    assert_eq!(scrolled[6], "… 5 under");
    let last = held(help(), 53, 7, usize::MAX);
    assert_eq!(last[1], "… 8 above");
    assert_eq!(
        last[6],
        "ctrl-c     leave: every run keeps going without this…"
    );
}

/// Given fewer rows than its sentence and a key, the list keeps to them: its sentence alone,
/// and nothing in none.
#[test]
fn a_list_given_too_few_rows_keeps_to_them() {
    assert_eq!(
        held(help(), 53, 1, 0),
        ["You never have to remember more than this."]
    );
    assert!(held(help(), 53, 0, 0).is_empty());
}
