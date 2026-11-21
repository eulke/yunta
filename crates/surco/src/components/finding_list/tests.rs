//! A finding list is a block every surface draws: as Markdown, as a page, and on a terminal in
//! every environment, a finding a row with its severity's word in one column, and the answer of
//! one answered under its title.

use std::path::{Path, PathBuf};

use super::{FindingList, FindingRow};
use crate::doc::Doc;
use crate::glyphs::Mark;
use crate::golden::{assert_golden, Environment, ENVIRONMENTS};
use crate::line::{Fill, Line, Tone};
use crate::look::{Look, Width};
use crate::surface::{Html, Markdown, Surface, Terminal};
use crate::width::cell_width;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/finding_list")
}

/// The cells a list stands in where a screen draws it: a run's pane beside its nodes on a
/// terminal of 120 columns, the screen's width inside its margins at 80 columns and at 60.
const REGIONS: [usize; 3] = [84, 78, 58];

/// The four findings three reviewers posted on the design's run, in the order they posted
/// them, none answered yet.
fn posted() -> FindingList {
    FindingList::new(vec![
        FindingRow::new(
            Mark::Caution,
            "major",
            "Global pack paths ignore the configured YUNTA_HOME",
        ),
        FindingRow::new(
            Mark::Caution,
            "major",
            "Global pack commands ignore the configured YUNTA_HOME state root",
        ),
        FindingRow::new(
            Mark::Absent,
            "minor",
            "Pack audit cannot inspect globally installed packs",
        ),
        FindingRow::new(
            Mark::Failed,
            "blocking",
            "Pack target API shapes are defined outside the declared file",
        ),
    ])
    .pointing_at(0)
}

/// The same findings once the first and the last are answered, the pointer on the first
/// one still open.
pub(crate) fn answered() -> FindingList {
    let mut list = posted().pointing_at(1);
    list.rows[0] = list.rows[0].clone().answered(
        "fixed: Global pack paths now use the shared user_state_root resolver, which honors YUNTA_HOME.",
    );
    list.rows[3] = list.rows[3].clone().answered(
        "fixed: PackTarget and its path functions are now defined in crates/cli/src/pack.rs.",
    );
    list
}

/// `list` drawn `cells` wide in `environment`'s glyphs and paint, a line a row.
fn drawn(list: FindingList, environment: &Environment, cells: usize) -> Vec<Line> {
    let look = Look {
        width: Width::exact(cells),
        ..Look::of(environment)
    };
    Terminal::on(look).lines(&Doc::new().with(list))
}

/// Where a row's title starts: past its pointer's cells, its mark and its severity's word.
fn title_at(row: &Line, title: &str) -> usize {
    let text = row.text();
    let at = text.find(title).expect("the row holds its title");
    cell_width(&text[..at])
}

#[test]
fn an_answered_finding_draws_the_done_mark_and_keeps_its_severity_word() {
    let rows = drawn(answered(), &ENVIRONMENTS[2], 78);
    assert_eq!(
        rows[0].text(),
        "  ✓ major    Global pack paths ignore the configured YUNTA_HOME"
    );
    let mark = &rows[0].spans()[2];
    assert_eq!((mark.text.as_str(), mark.tone), ("✓", Tone::Done));
    assert_eq!(
        rows[1].text(),
        "             fixed: Global pack paths now use the shared user_state_root reso…",
        "its answer stands under its title, cut at the row's end"
    );
    assert!(rows[4].text().starts_with("  ✓ blocking Pack target"));
}

#[test]
fn the_severity_words_line_up_in_one_column() {
    let rows = drawn(posted(), &ENVIRONMENTS[2], 78);
    let titles: Vec<usize> = rows
        .iter()
        .zip(&posted().rows)
        .map(|(row, finding)| title_at(row, &finding.title))
        .collect();
    assert_eq!(titles, [13, 13, 13, 13], "{rows:#?}");
    // With no word as wide as `blocking`, the column is as wide as the widest there is.
    let mut open = posted();
    open.rows.pop();
    let rows = drawn(open, &ENVIRONMENTS[2], 78);
    assert_eq!(
        rows[2].text(),
        "  ◌ minor Pack audit cannot inspect globally installed packs"
    );
}

#[test]
fn the_pointed_finding_is_filled_and_the_others_keep_its_cells() {
    let rows = drawn(answered(), &ENVIRONMENTS[2], 78);
    let pointed = &rows[2];
    assert!(pointed.text().starts_with("▸ ▲ major"), "{pointed:?}");
    assert_eq!(pointed.fill(), Some(Fill::Selection));
    assert_eq!(pointed.spans()[0].tone, Tone::NeedsYou);
    assert!(pointed
        .spans()
        .iter()
        .any(|span| span.tone == Tone::Strong && span.text.starts_with("Global pack commands")));
    for other in [&rows[0], &rows[3], &rows[4]] {
        assert!(other.text().starts_with("  "), "{other:?}");
        assert_eq!(other.fill(), None);
    }
}

#[test]
fn a_finding_list_matches_its_goldens_on_every_surface() {
    let doc = Doc::new().with(answered());
    assert_golden(&goldens().join("answered.md"), &Markdown.draw(&doc));
    assert_golden(&goldens().join("answered.html"), &Html.body(&doc));
    for (case, list) in [
        ("posted", posted as fn() -> FindingList),
        ("answered", answered),
    ] {
        for environment in &ENVIRONMENTS {
            let blocks: Vec<String> = REGIONS
                .into_iter()
                .map(|cells| {
                    drawn(list(), environment, cells)
                        .iter()
                        .map(|row| format!("{}\n", environment.paint.line(row)))
                        .collect()
                })
                .collect();
            assert_golden(&environment.golden(&goldens(), case), &blocks.join("\n"));
        }
    }
}
