//! What is open over a screen, boxed: round corners, its title in the top edge, its lines
//! inside the uprights, as tall as what it holds, and scrolled inside the box when it holds
//! more; a palette's rows in their three columns where its box holds them, and help's keys.

use std::path::{Path, PathBuf};

use crate::components::{ChoiceList, ChoiceRow, Choices, KeyList, Legend, Overlay, Sight};
use crate::doc::Doc;
use crate::golden::{assert_golden, Environment, ENVIRONMENTS};
use crate::line::{Fill, Line};
use crate::look::{Look, Width};
use crate::surface::Terminal;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/boxed")
}

/// A screen `cells` wide inside its margins, a box stands over, in `environment`'s glyphs
/// and paint.
fn over(environment: &Environment, cells: usize) -> Terminal {
    Terminal::on(Look {
        width: Width::exact(cells),
        ..Look::of(environment)
    })
}

/// What help lists over a run: four keys, each with what it does.
fn keys() -> Overlay {
    let lines = [
        "/      anything: only what can be done here",
        "n      start a new run, from anywhere",
        "esc    one step back; what it leaves stays waiting",
        "q      leave: every run keeps going",
    ]
    .map(|line| Line::new().plain(line));
    Overlay {
        title: "keys".to_string(),
        doc: Doc::new().with(lines.to_vec()),
        legend: Legend::new().key("esc", "back"),
        sight: Sight::default(),
    }
}

/// `count` lines to read, `line 1` to `line <count>`, from the line `top`.
fn many(count: usize, top: usize) -> Overlay {
    let lines: Vec<Line> = (1..=count)
        .map(|at| Line::new().plain(format!("line {at}")))
        .collect();
    Overlay {
        title: "palette".to_string(),
        doc: Doc::new().with(lines),
        legend: Legend::new().key("esc", "back"),
        sight: Sight::from_line(top),
    }
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::text).collect()
}

/// A box's corners are the round ones, its title in the top edge a cell after the first,
/// its words a cell in from the left upright, and its last row the bottom edge.
#[test]
fn a_box_has_round_corners_and_ends_on_its_bottom_edge() {
    let boxed = over(&ENVIRONMENTS[2], 58).boxed(&keys(), 56, 10);
    assert_eq!(
        texts(&boxed.lines),
        [
            "╭ keys ────────────────────────────────────────────────╮",
            "│ /      anything: only what can be done here          │",
            "│ n      start a new run, from anywhere                │",
            "│ esc    one step back; what it leaves stays waiting   │",
            "│ q      leave: every run keeps going                  │",
            "╰──────────────────────────────────────────────────────╯",
        ]
    );
    assert_eq!(boxed.cursor, None);
}

/// In ASCII the box is drawn in its marks.
#[test]
fn in_ascii_the_box_is_drawn_in_its_marks() {
    let boxed = over(&ENVIRONMENTS[1], 20).boxed(&many(1, 0), 20, 10);
    assert_eq!(
        texts(&boxed.lines),
        [
            ". palette ---------.",
            "| line 1           |",
            "'------------------'",
        ]
    );
}

/// What holds more lines than the box has rows scrolls inside it, counting what is below.
#[test]
fn what_holds_more_than_the_box_scrolls_inside_it() {
    let boxed = over(&ENVIRONMENTS[2], 30).boxed(&many(20, 3), 30, 8);
    let text = texts(&boxed.lines);
    assert_eq!(text.len(), 8, "{text:#?}");
    assert_eq!(text[1], "│ line 4                     │", "{text:#?}");
    assert!(text[6].contains("more lines"), "{text:#?}");
    assert_eq!(boxed.laid.top, 3);
    assert!(boxed.laid.scrolls());
}

/// The palette over a run at work: what each act does, its key where the run's screen takes
/// one, and the command that does the same, the first pointed at; nothing typed yet, and no
/// keys of its own, which the screen's legend holds while it is open.
fn palette() -> Overlay {
    // What each act does, its key, and its command, past `yunta ` and before the run's handle.
    let acts = [
        ("stop it and everything under it", "c", "cancel"),
        ("what it cost, and where its time went", "s", "stats"),
        ("draw its graph", "g", "graph --run"),
        ("the whole page, in a browser", "o", "open"),
        ("the run's whole chronicle", "", "status Q78GRR --follow"),
        ("start a new run", "n", "run <workflow>"),
        ("every run of this repository", "", "list --runs"),
        ("the workflows you can run", "", "list"),
        ("check adapters, git, forge, packs", "", "doctor"),
        ("remove orphaned checkouts", "", "gc"),
        ("the shape of every document yunta reads", "", "schema"),
    ];
    let of_the_run = ["cancel", "stats", "graph --run", "open"];
    let rows = acts
        .into_iter()
        .map(|(does, key, command)| ChoiceRow {
            name: does.to_string(),
            said: Some(match of_the_run.contains(&command) {
                true => format!("yunta {command} Q78GRR"),
                false => format!("yunta {command}"),
            }),
            cost: None,
            key: (!key.is_empty()).then(|| key.to_string()),
        })
        .collect();
    let list = ChoiceList {
        choices: Some(Choices { rows, at: 0 }),
        filter: None,
        legend: Legend::new(),
    };
    Overlay {
        title: "what do you want to do?".to_string(),
        doc: Doc::new().with(list),
        legend: Legend::new()
            .arrows("move")
            .key("enter", "do it")
            .key("esc", "back"),
        sight: Sight::following_pointer(),
    }
}

/// Help over a run: its sentence, then the keys every screen answers to and the run's,
/// `pgup pgdn` the widest.
fn help() -> Overlay {
    let keys = KeyList {
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
    };
    Overlay {
        title: "keys".to_string(),
        doc: Doc::new().with(keys),
        legend: Legend::new().key("esc", "back"),
        sight: Sight::default(),
    }
}

/// The palette's box on a terminal of 100 columns and wider, 84 cells, holds a row's three
/// columns: what it does, its key and its command; on one of 80, 76 cells, what it does alone.
#[test]
fn a_palette_row_holds_its_key_and_its_command_from_a_terminal_of_100_columns() {
    let wide = over(&ENVIRONMENTS[2], 98).boxed(&palette(), 84, 15);
    assert_eq!(
        wide.lines[3].text(),
        "│▸ stop it and everything under it         c     yunta cancel Q78GRR               │"
    );
    let narrow = over(&ENVIRONMENTS[2], 78).boxed(&palette(), 76, 15);
    assert_eq!(
        narrow.lines[3].text(),
        format!("│▸ stop it and everything under it{}│", " ".repeat(41))
    );
    assert!(narrow
        .lines
        .iter()
        .all(|line| !line.text().contains("Q78GRR")));
}

/// A palette's box as wide as on the design's wide screen, 84 cells, over a terminal of 96
/// columns holds what each act does alone: its keys and commands stand from 100 columns, as
/// the legend's far keys do, whatever room the box has.
#[test]
fn a_palette_row_holds_what_it_does_alone_under_a_terminal_of_100_columns() {
    let boxed = over(&ENVIRONMENTS[2], 94).boxed(&palette(), 84, 15);
    assert_eq!(
        boxed.lines[3].text(),
        format!("│▸ stop it and everything under it{}│", " ".repeat(49))
    );
    assert!(boxed
        .lines
        .iter()
        .all(|line| !line.text().contains("Q78GRR")));
}

/// The pointed row stands on the selection from one upright to the other.
#[test]
fn the_pointed_row_is_filled_to_the_box_s_edges() {
    let boxed = over(&ENVIRONMENTS[2], 98).boxed(&palette(), 84, 15);
    let pointed = &boxed.lines[3];
    let spans = pointed.spans();
    let inside = &spans[1..spans.len() - 1];
    assert!(
        inside.iter().all(|span| span.fill == Some(Fill::Selection)),
        "{pointed:?}"
    );
    assert!(boxed.lines[4]
        .spans()
        .iter()
        .all(|span| span.fill.is_none()));
}

/// A box a case draws: what is open, and the cells and rows a terminal of 120, 80 and 60
/// columns gives it, as the targets draw it.
struct Case {
    name: &'static str,
    overlay: fn() -> Overlay,
    sizes: [(usize, usize); 3],
}

/// The cells inside the margins of a terminal of 120, 80 and 60 columns, the screens a
/// case's boxes stand over.
const SCREENS: [usize; 3] = [118, 78, 58];

/// The palette's box and help's, at the sizes a terminal of 120, 80 and 60 columns gives
/// them, and a box scrolled inside, in every environment.
#[test]
fn a_box_matches_its_goldens_in_every_environment() {
    let cases = [
        Case {
            name: "palette",
            overlay: palette,
            sizes: [(84, 15), (76, 15), (56, 9)],
        },
        Case {
            name: "help",
            overlay: help,
            sizes: [(70, 19), (70, 19), (56, 9)],
        },
    ];
    for environment in &ENVIRONMENTS {
        for Case {
            name: case,
            overlay,
            sizes,
        } in &cases
        {
            let drawn: Vec<String> = sizes
                .iter()
                .zip(SCREENS)
                .map(|((cells, rows), screen)| {
                    painted(
                        environment,
                        over(environment, screen)
                            .boxed(&overlay(), *cells, *rows)
                            .lines,
                    )
                })
                .collect();
            assert_golden(&environment.golden(&goldens(), case), &drawn.join("\n"));
        }
        let cells = (environment.width - 4).min(60);
        let scrolled = over(environment, cells).boxed(&many(20, 3), cells, 8);
        assert_golden(
            &environment.golden(&goldens(), "scrolled"),
            &painted(environment, scrolled.lines),
        );
    }
}

/// `lines` painted in `environment`'s paint, a line a row.
fn painted(environment: &Environment, lines: Vec<Line>) -> String {
    lines
        .iter()
        .map(|line| format!("{}\n", environment.paint.line(line)))
        .collect()
}
