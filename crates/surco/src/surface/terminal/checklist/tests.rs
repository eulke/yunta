//! A checklist's row as a receipt's pane draws it: its mark, its subject in a column as wide as
//! the widest subject and two cells, then what it found; and a checklist that folds the checks
//! that hold, set apart from the rest or close under its fold.

use std::path::{Path, PathBuf};

use crate::components::{Checklist, Found};
use crate::doc::Doc;
use crate::glyphs::Glyphs;
use crate::golden::assert_golden;
use crate::look::{Look, Width};
use crate::paint::Paint;
use crate::surface::{Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/checklist")
}

/// The cells of a run's pane beside its nodes, on a terminal of 120 columns.
const PANE: usize = 84;

/// The five checks of the design's receipt, each holding.
fn receipt() -> Checklist {
    let mut checks = Checklist::default();
    checks.push(
        Found::Holds,
        "criteria",
        "1 of 1 green, red before the work",
    );
    checks.push(
        Found::Holds,
        "baseline",
        "cargo test --workspace still passes",
    );
    checks.push(Found::Holds, "scope", "6 files, 2 of them granted by you");
    checks.push(Found::Holds, "review", "4 findings, 4 answered");
    checks.push(Found::Holds, "event chain", "intact");
    checks
}

/// The five checks of a run that finished holding a finding that blocks: two with nothing to
/// check, and the review failed.
fn reported() -> Checklist {
    let mut checks = Checklist::default();
    checks.push(
        Found::NotApplicable,
        "criteria",
        "none declared, nothing to prove",
    );
    checks.push(
        Found::NotApplicable,
        "baseline",
        "not used by this workflow",
    );
    checks.push(Found::Holds, "scope", "no file touched");
    checks.push(Found::Problem, "review", "1 finding, 0 answered");
    checks.push(Found::Holds, "event chain", "intact");
    checks
}

#[test]
fn a_receipt_s_checklist_draws_the_design_s_rows_in_its_pane() {
    for (name, glyphs) in [
        ("unicode-120", Glyphs::Unicode),
        ("ascii-120", Glyphs::Ascii),
    ] {
        let look = Look {
            glyphs,
            paint: Paint::Plain,
            width: Width::exact(PANE),
            motion: crate::motion::Motion::Still,
        };
        let doc = Doc::new().with(receipt()).with(reported());
        let drawn = Terminal::on(look).draw(&doc);
        assert_golden(&goldens().join(format!("receipt.{name}.txt")), &drawn);
    }
}

/// A list that folds sets what does not hold a line apart from its fold, as a report prints
/// it; close under its fold, as a screen's one list of what one thing answers for, right
/// under it.
#[test]
fn a_list_close_under_its_fold_sets_what_does_not_hold_right_under_it() {
    let drawn = |close: bool| {
        let list = Checklist::default().folded("healthy");
        let mut list = match close {
            true => list.close_under_its_fold(),
            false => list,
        };
        list.push(Found::Holds, "git", "commits as Ada <ada@example.com>");
        list.push(Found::Caution, "runners", "none declared");
        let drawn = Terminal::on(Look::plain()).draw(&Doc::new().with(list));
        drawn.lines().map(str::to_string).collect::<Vec<_>>()
    };
    let apart = ["  + 1 healthy  git", "", "  ^ runners    none declared"];
    assert_eq!(drawn(false), apart);
    let close = ["  + 1 healthy  git", "  ^ runners    none declared"];
    assert_eq!(drawn(true), close);
}

/// A list that sets its checks on their rows stands what each found whole on one row, cut at
/// the line's end where it is wider, where a list that wraps carries it on under itself: a
/// command a person reads by how it starts.
#[test]
fn a_list_on_its_rows_cuts_what_a_check_found_where_a_list_wraps_it() {
    let command = "`cargo test --workspace --test pack_global_spec global_pack_commands` exits 0";
    let drawn = |on_their_rows: bool| {
        let list = Checklist::default();
        let mut list = match on_their_rows {
            true => list.on_their_rows(),
            false => list,
        };
        list.push(Found::Holds, "global-pack-install", command);
        let look = Look {
            width: Width::exact(60),
            ..Look::plain()
        };
        let drawn = Terminal::on(look).draw(&Doc::new().with(list));
        drawn.lines().map(str::to_string).collect::<Vec<_>>()
    };
    assert_eq!(drawn(false).len(), 3, "{:#?}", drawn(false));
    let cut = drawn(true);
    assert_eq!(cut.len(), 1, "{cut:#?}");
    assert!(cut[0].ends_with("..."), "{cut:#?}");
    assert!(cut[0].contains("`cargo test --workspace"), "{cut:#?}");
}
