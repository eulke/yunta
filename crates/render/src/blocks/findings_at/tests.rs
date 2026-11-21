//! The run's findings at the one pointed at, as a pane shows them: their heading, which the
//! pane is titled by, a finding a row with the pointer on the one pointed at, then that one's
//! node and place, its words wrapped under them, and its answer apart once it has one.

use surco::doc::Doc;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::surface::Terminal;
use yunta_core::events::findings::RunFindings;
use yunta_core::{FindingId, NodeId};

use super::FindingsAt;

/// Two findings two reviewers posted, the second answered.
const POSTED: &str = r#"
findings:
  - node: review@reviewer
    id: global-paths-ignore-yunta-home
    severity: major
    title: Global pack paths ignore the configured YUNTA_HOME
    location: crates/cli/src/pack/target.rs:31-35
    detail: global_data_dir uses only Env.home and appends /.yunta. With HOME=/home/alice and YUNTA_HOME=/scratch/yunta-state, pack add --global stores under /home/alice/.yunta/packs.
  - node: conform
    id: target-outside-declared-file
    severity: blocking
    title: Pack target API shapes are defined outside the declared file
    location: crates/cli/src/pack.rs:40
    detail: The plan declares them in crates/cli/src/pack/target.rs.
    answers:
      - { by: fix-findings, answer: fixed, why: PackTarget now stands in pack.rs }
"#;

/// `blocks` laid out as a pane 84 cells wide draws them, in Unicode: each row's text.
fn drawn(blocks: Vec<surco::doc::Block>) -> Vec<String> {
    let look = Look {
        glyphs: Glyphs::Unicode,
        width: Width::exact(84),
        ..Look::plain()
    };
    let lines = Terminal::on(look).lines(&Doc::new().with_all(blocks));
    lines.iter().map(|line| line.text()).collect()
}

#[test]
fn the_finding_pointed_at_stands_under_the_list_with_its_node_its_place_and_its_words() {
    let view: RunFindings = yunta_core::yaml::parse(POSTED).unwrap();
    let (node, id) = (
        NodeId::from("review@reviewer"),
        FindingId::from("global-paths-ignore-yunta-home"),
    );
    let at = FindingsAt::of(&view, Some((Some(&node), &id)));
    assert_eq!(
        at.heading(Glyphs::Unicode),
        "findings · 1 of 2 answered · review@reviewer, conform"
    );
    assert_eq!(
        drawn(at.blocks(Glyphs::Unicode)),
        [
            "▸ ▲ major    Global pack paths ignore the configured YUNTA_HOME",
            "  ✓ blocking Pack target API shapes are defined outside the declared file",
            "             fixed: PackTarget now stands in pack.rs",
            "",
            "review@reviewer      crates/cli/src/pack/target.rs:31-35",
            "global_data_dir uses only Env.home and appends /.yunta. With HOME=/home/alice and",
            "YUNTA_HOME=/scratch/yunta-state, pack add --global stores under",
            "/home/alice/.yunta/packs.",
        ]
    );
}

#[test]
fn an_answered_finding_pointed_at_says_its_answer_apart_led_by_the_done_mark() {
    let view: RunFindings = yunta_core::yaml::parse(POSTED).unwrap();
    let (node, id) = (
        NodeId::from("conform"),
        FindingId::from("target-outside-declared-file"),
    );
    let at = FindingsAt::of(&view, Some((Some(&node), &id)));
    assert_eq!(
        drawn(at.pointed(Glyphs::Unicode)),
        [
            "conform              crates/cli/src/pack.rs:40",
            "The plan declares them in crates/cli/src/pack/target.rs.",
            "",
            "✓ fixed: PackTarget now stands in pack.rs",
        ]
    );
    let none = FindingsAt::of(&view, None);
    assert!(none.pointed(Glyphs::Unicode).is_empty(), "none pointed at");
}
