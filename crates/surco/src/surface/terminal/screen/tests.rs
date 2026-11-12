//! A screen's rows that are no document: the trail with the tally kept whole at its end,
//! the rule across the width, and a title strong or muted by where the keys act.

use std::path::{Path, PathBuf};

use crate::components::{Attention, Count, Legend};
use crate::golden::{assert_golden, Environment, ENVIRONMENTS};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::surface::Terminal;
use crate::width::cell_width;
use crate::words::Standing;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/screen")
}

/// A terminal whose lines are exactly `cells` wide, in `environment`'s glyphs and paint.
fn on(environment: &Environment, cells: usize) -> Terminal {
    Terminal::on(Look {
        width: Width::exact(cells),
        ..Look::of(environment)
    })
}

fn places(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// Thirteen runs that need the person and one stalled.
fn tally() -> Vec<Count> {
    vec![
        Count {
            standing: Standing::NeedsYou,
            runs: 13,
        },
        Count {
            standing: Standing::Stalled,
            runs: 1,
        },
    ]
}

/// The reference's trail row, inside its margins: 78 cells in ASCII, 58 in Unicode.
#[test]
fn the_trail_stands_at_the_start_and_the_tally_at_the_end() {
    let trail = places(&["yunta", "Q78GRR", "plan"]);
    let ascii = on(&ENVIRONMENTS[1], 78).trail(&trail, &tally());
    assert_eq!(
        ascii.text(),
        "yunta :: Q78GRR :: plan                            ? 13 need you ; ^ 1 stalled"
    );
    let unicode = on(&ENVIRONMENTS[2], 58).trail(&trail, &tally());
    assert_eq!(
        unicode.text(),
        "yunta › Q78GRR › plan          ◆ 13 need you · ▲ 1 stalled"
    );
}

/// A row too short for the whole trail cuts the trail and keeps the tally whole.
#[test]
fn a_short_row_cuts_the_trail_and_keeps_the_tally() {
    let trail = places(&["yunta", "Q78GRR", "implement", "the third task"]);
    let row = on(&ENVIRONMENTS[2], 58).trail(&trail, &tally());
    let text = row.text();
    assert_eq!(cell_width(&text), 58, "{text}");
    assert!(text.starts_with("yunta › Q78GRR"), "{text}");
    assert!(text.contains('…'), "{text}");
    assert!(text.ends_with("◆ 13 need you · ▲ 1 stalled"), "{text}");
}

/// With nothing counted, the trail stands alone.
#[test]
fn with_nothing_counted_the_trail_stands_alone() {
    let row = on(&ENVIRONMENTS[2], 58).trail(&places(&["yunta"]), &[]);
    assert_eq!(row.text(), "yunta");
}

/// The rule is the furrow of no node: every cell the ground not yet plowed.
#[test]
fn the_rule_takes_every_cell_with_the_ground_not_yet_plowed() {
    assert_eq!(on(&ENVIRONMENTS[2], 60).rule().text(), "┄".repeat(60));
    assert_eq!(on(&ENVIRONMENTS[1], 80).rule().text(), ".".repeat(80));
}

fn tones(line: &Line) -> Vec<Tone> {
    line.spans().iter().map(|span| span.tone).collect()
}

/// The edge between two regions is an upright with a cell either side, faint.
#[test]
fn the_edge_is_an_upright_between_two_cells() {
    let unicode = on(&ENVIRONMENTS[2], 60).edge();
    assert_eq!(unicode.text(), " │ ");
    assert_eq!(tones(&unicode), [Tone::Faint]);
    assert_eq!(on(&ENVIRONMENTS[1], 80).edge().text(), " | ");
}

/// A title is strong where the keys act on its region, and muted elsewhere.
#[test]
fn a_title_is_strong_where_the_keys_act_and_muted_elsewhere() {
    let terminal = on(&ENVIRONMENTS[2], 22);
    assert_eq!(terminal.title("nodes", true).text(), "nodes");
    assert_eq!(tones(&terminal.title("nodes", true)), [Tone::Strong]);
    assert_eq!(tones(&terminal.title("nodes", false)), [Tone::Muted]);
}

/// A title wider than its region is cut.
#[test]
fn a_title_wider_than_its_region_is_cut() {
    let title = on(&ENVIRONMENTS[2], 10).title("what the run hands over", true);
    assert_eq!(title.text(), "what the…");
}

/// The trail, the rule, two titles, the attention and the legend, at the reference's
/// widths and in every environment.
#[test]
fn the_rows_match_their_goldens_in_every_environment() {
    let attention = Attention::key("implement asks at its terminal", "a", "go there").of("EKPBGE");
    let legend = Legend::new()
        .arrows("node")
        .key("s", "stats")
        .key("g", "graph")
        .key("q", "field, it keeps going");
    for environment in &ENVIRONMENTS {
        let inner = environment.width - 2;
        let rows = [
            on(environment, inner).trail(&places(&["yunta", "Q78GRR", "plan"]), &tally()),
            on(environment, environment.width).rule(),
            on(environment, 22).title("nodes", false),
            on(environment, environment.width).edge(),
            on(environment, inner).title("plan", true),
            on(environment, inner).attention(&attention),
            on(environment, inner).legend(&legend),
        ];
        let terminal = on(environment, environment.width);
        let drawn: String = rows
            .iter()
            .map(|line| format!("{}\n", terminal.look.paint.line(line)))
            .collect();
        assert_golden(&environment.golden(&goldens(), "rows"), &drawn);
    }
}
