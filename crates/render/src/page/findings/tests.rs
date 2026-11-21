//! The run's findings as a run's screen lists them: under their heading, a finding a row in
//! the order they were posted, an answered one keeping its row with its answer under it.

use surco::doc::Block;
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use yunta_core::events::findings::RunFindings;

use crate::page::{Findings, Page};

/// Four findings three reviewers posted, none answered yet, in the order they were posted.
const POSTED: &str = r#"
findings:
  - node: review@reviewer
    id: global-paths-ignore-yunta-home
    severity: major
    title: Global pack paths ignore the configured YUNTA_HOME
    location: crates/cli/src/pack/target.rs:31-35
    detail: global_data_dir uses only Env.home and appends /.yunta.
  - node: review@reviewer-alt
    id: global-commands-ignore-yunta-home
    severity: major
    title: Global pack commands ignore the configured YUNTA_HOME state root
    location: crates/cli/src/pack/target.rs:29-33
    detail: Other user-level state resolves beneath YUNTA_HOME.
  - node: review@reviewer-alt
    id: audit-cannot-inspect-global
    severity: minor
    title: Pack audit cannot inspect globally installed packs
    location: crates/cli/src/commands/pack_audit.rs:12
    detail: The audit reads the project's packs alone.
  - node: conform
    id: target-outside-declared-file
    severity: blocking
    title: Pack target API shapes are defined outside the declared file
    location: crates/cli/src/pack.rs:40
    detail: The plan declares them in crates/cli/src/pack/target.rs.
"#;

/// The run's findings, three reviewers having posted four and none answered, read under
/// their heading, a finding a row in the order they were posted.
#[test]
fn the_run_s_findings_read_under_their_heading_a_finding_a_row() {
    let view: RunFindings = yunta_core::yaml::parse(POSTED).unwrap();
    let look = Look {
        glyphs: surco::glyphs::Glyphs::Unicode,
        ..Look::plain()
    };
    let doc = Findings::Run(&view).document(&look);
    let drawn = Terminal::on(look).draw(&doc);
    let heading = "findings · 0 of 4 answered · review@reviewer, review@reviewer-alt, conform";
    assert!(drawn.contains(heading), "{drawn}");
    let rows = [
        "  ▲ major    Global pack paths ignore the configured YUNTA_HOME",
        "  ▲ major    Global pack commands ignore the configured YUNTA_HOME state root",
        "  ◌ minor    Pack audit cannot inspect globally installed packs",
        "  ✗ blocking Pack target API shapes are defined outside the declared file",
    ];
    assert!(drawn.contains(&rows.join("\n")), "{drawn}");
    let listed = doc
        .blocks()
        .iter()
        .filter(|block| matches!(block, Block::Findings(_)));
    assert_eq!(listed.count(), 1, "{drawn}");
}

/// A finding a node answered keeps its row, the done mark leading it and the answer under
/// its title, and the heading counts it answered.
#[test]
fn an_answered_finding_keeps_its_row_with_its_answer_under_it() {
    let answered = POSTED.replacen(
        "    detail: global_data_dir uses only Env.home and appends /.yunta.\n",
        "    detail: global_data_dir uses only Env.home and appends /.yunta.\n    answers:\n      \
         - { by: fix-findings, answer: fixed, why: Global pack paths now use the shared \
         resolver }\n",
        1,
    );
    let view: RunFindings = yunta_core::yaml::parse(&answered).unwrap();
    let look = Look {
        glyphs: surco::glyphs::Glyphs::Unicode,
        ..Look::plain()
    };
    let drawn = Terminal::on(look).draw(&Findings::Run(&view).document(&look));
    let heading = "findings · 1 of 4 answered · review@reviewer, review@reviewer-alt, conform";
    assert!(drawn.contains(heading), "{drawn}");
    let row = "  ✓ major    Global pack paths ignore the configured YUNTA_HOME\n             \
               fixed: Global pack paths now use the shared resolver";
    assert!(drawn.contains(row), "{drawn}");
}
