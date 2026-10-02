//! The children a run bore, as the rows a person reads them in: each
//! under the node that bore it, saying where it stands and which run it
//! is — a tree, never a number.

use yunta_core::events::TerminalState;
use yunta_core::NodeId;
use yunta_engine::{ChildLink, RunFrame};

use crate::render::{Glyphs, Mark};

/// What a child run whose link the log recorded under no node is filed
/// under: what the log does not say, rather than a node it might not
/// belong to.
const NO_NODE: &str = "under no node on this run's log";

/// The row one child run takes under the node that bore it: where the
/// parent's log says it stands, and which run to go and look at.
///
/// What it does not say is which workflow the child ran. No event
/// carries a child's name — `child_run_created` records the link and
/// the child's workflow hash — and naming one means reading the child's
/// own frozen manifest, which is a read no frame does. The row says
/// what this run's log says, and nothing it would have to guess.
///
/// Where the child stands comes first, so a row cut to a narrow
/// terminal still says a child is open and loses only which one. What
/// it never becomes is a number: heterogeneous children averaged into
/// one figure is the lying percentage under another name.
pub(in crate::surface) fn child_row(child: &ChildLink, glyphs: Glyphs) -> String {
    let standing = child_standing(child.terminal);
    format!(
        "{} {} · child run {}",
        glyphs.mark(standing.0),
        standing.1,
        child.run_id.handle()
    )
}

/// Every child this run bore, grouped under the node that bore it, each
/// group in the order the parent's log recorded it.
pub(in crate::surface) fn children_by_node(frame: &RunFrame) -> Vec<(String, Vec<&ChildLink>)> {
    let mut groups: Vec<(String, Vec<&ChildLink>)> = Vec::new();
    for child in &frame.children {
        let parent = match &child.node {
            Some(node) => format!("node `{node}`"),
            None => NO_NODE.to_string(),
        };
        match groups.iter_mut().find(|(under, _)| under == &parent) {
            Some((_, born)) => born.push(child),
            None => groups.push((parent, vec![child])),
        }
    }
    groups
}

/// The children `node` bore, in the order the parent's log recorded
/// them.
pub(super) fn children_of<'a>(frame: &'a RunFrame, node: &NodeId) -> Vec<&'a ChildLink> {
    frame
        .children
        .iter()
        .filter(|child| child.node.as_ref() == Some(node))
        .collect()
}

/// Where a child stands, as the mark that carries it for the eye and
/// the word that carries it for everybody.
///
/// A child the parent's log has no close for is open and nothing more:
/// how far it has got is on the child's own log, which this run never
/// opens.
pub(in crate::surface) fn child_standing(terminal: Option<TerminalState>) -> (Mark, &'static str) {
    match terminal {
        None => (Mark::Running, "still open"),
        Some(TerminalState::Done) => (Mark::Done, closed_as(TerminalState::Done)),
        Some(TerminalState::Failed) => (Mark::Failed, closed_as(TerminalState::Failed)),
        Some(TerminalState::Cancelled) => (Mark::Failed, closed_as(TerminalState::Cancelled)),
        Some(TerminalState::Promoted) => (Mark::Reroute, closed_as(TerminalState::Promoted)),
    }
}

/// The word a run's close is named by, wherever a surface names one:
/// the child under the node that bore it, and the event line that
/// records either.
///
/// A run's terminal state is its own vocabulary, beside the one a
/// node's state is read in ([`crate::render::StateWord`]) — a run
/// promotes and a node never does — so it is said here, once, for every
/// surface that says it.
pub(in crate::surface) fn closed_as(state: TerminalState) -> &'static str {
    match state {
        TerminalState::Done => "finished",
        TerminalState::Failed => "failed",
        TerminalState::Cancelled => "cancelled",
        TerminalState::Promoted => "promoted",
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::RunId;
    use yunta_testkit::{child_link, run_frame};

    use super::*;

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const FIRST: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P6");
    const SECOND: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P7");

    /// The link `run_id` left on this run's log: born under `node`, and
    /// closed `terminal` or still open.
    fn child(
        run_id: &RunId,
        node: Option<&'static str>,
        terminal: Option<TerminalState>,
    ) -> ChildLink {
        child_link(run_id, node.map(NodeId::from_static).as_ref(), terminal)
    }

    /// A run that bore `children` and declares no nodes: what these
    /// tests read is the children, and the rest of the frame is the
    /// base every surface test shares.
    fn composed(children: Vec<ChildLink>) -> RunFrame {
        RunFrame {
            children,
            ..run_frame(&RUN)
        }
    }

    #[test]
    fn every_child_is_grouped_under_the_node_that_bore_it() {
        let frame = composed(vec![
            child(&FIRST, Some("compose"), None),
            child(&SECOND, Some("compose"), Some(TerminalState::Done)),
        ]);
        let grouped = children_by_node(&frame);
        assert_eq!(grouped.len(), 1, "{grouped:?}");
        let (under, born) = grouped.first().expect("the one group");
        assert_eq!(under, "node `compose`");
        assert_eq!(
            born.iter().map(|child| &child.run_id).collect::<Vec<_>>(),
            vec![&FIRST, &SECOND],
            "in the order the parent's log recorded them"
        );
    }

    #[test]
    fn a_child_the_log_recorded_under_no_node_is_said_to_have_none() {
        let frame = composed(vec![child(&FIRST, None, None)]);
        let grouped = children_by_node(&frame);
        assert_eq!(
            grouped.first().map(|(under, _)| under.as_str()),
            Some(NO_NODE),
            "a group named for what the log does not say, never for a node it might not \
             belong to: {grouped:?}"
        );
    }

    #[test]
    fn a_child_row_says_where_the_child_stands_before_which_child_it_is() {
        let open = child(&FIRST, Some("compose"), None);
        let row = child_row(&open, Glyphs::Ascii);
        assert_eq!(row, format!("> still open · child run {}", FIRST.handle()));
        assert!(
            row.find("still open") < row.find(FIRST.handle()),
            "a row cut to a narrow terminal loses which child, never that there is one: {row}"
        );
    }

    #[test]
    fn every_way_a_child_can_close_is_read_as_the_word_its_own_run_closed_with() {
        for (terminal, word) in [
            (TerminalState::Done, "finished"),
            (TerminalState::Failed, "failed"),
            (TerminalState::Cancelled, "cancelled"),
            (TerminalState::Promoted, "promoted"),
        ] {
            let closed = child(&FIRST, Some("compose"), Some(terminal));
            assert!(
                child_row(&closed, Glyphs::Ascii).contains(word),
                "{terminal:?} reads as something other than `{word}`"
            );
        }
    }
}
