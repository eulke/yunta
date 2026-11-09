//! A decision on a failed node, beside the evidence its page quotes: what
//! failed is said once, above the decision where the node stands failed,
//! and in the decision's record where a person is being asked about it at a
//! terminal, which the page quotes nothing above for.

use surco::doc::Depth;
use surco::look::Look;
use surco::surface::{Surface, Terminal};
use surco::words::Standing;
use yunta_replay::frame::NodeFrame;
use yunta_replay::RunState;

use super::super::fixtures::{
    ahead, failed_lint, finished, frame, lint_decision, parked_on_lint, places, RUN,
};
use super::super::{Run, RunScope};
use crate::escalation::Decision;
use crate::page::Page;

/// The status page of a run parked on the decision about `lint`, which
/// stands as `lint` says, as a terminal with neither color nor Unicode
/// draws it.
fn status_with(lint: NodeFrame) -> String {
    let frame = frame(
        parked_on_lint(),
        vec![finished("build"), lint, ahead("ship")],
        0,
    );
    let state = RunState::default();
    let (on, escalation) = lint_decision();
    let page = Run {
        scope: RunScope::Status,
        depth: Depth::Glance,
        run_id: &RUN,
        frame: &frame,
        state: &state,
        places: places(),
        decision: Some(Decision {
            on: &on,
            escalation: &escalation,
            shown: &[],
        }),
        plans: Vec::new(),
    };
    let look = Look::plain();
    Terminal::on(look).draw(&page.document(&look))
}

#[test]
fn what_a_failed_node_said_is_quoted_once_whether_it_stands_failed_or_needs_a_person() {
    let failed = status_with(failed_lint());
    let asked = status_with(NodeFrame {
        standing: Standing::NeedsYou,
        ..failed_lint()
    });
    let said = "error[E0425]: cannot find value `x` in this scope";
    // Standing failed, the node's evidence is quoted above the decision,
    // which leaves its record out.
    assert_eq!(failed.matches(said).count(), 1, "{failed}");
    assert!(failed.contains("! lint ; exit 101"), "{failed}");
    // Asked about at a terminal, the node stands `needs you` and nothing
    // above quotes it: the decision's record says what failed.
    assert_eq!(asked.matches(said).count(), 1, "{asked}");
    assert!(!asked.contains("! lint ; exit 101"), "{asked}");
}
