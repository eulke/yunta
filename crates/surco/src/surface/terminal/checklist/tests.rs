//! A checklist's row as a receipt's pane draws it: its mark, its subject in a column as wide as
//! the widest subject and two cells, then what it found.

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
