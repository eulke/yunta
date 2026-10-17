//! Whether the run's tree moved since a node's failure left it: the answer
//! a failure's menu reads for its `tree_moved` fact.

use yunta_core::{NodeId, Seq, TreeId};

use super::RunState;

impl RunState {
    /// The run's tree as its newest record says it stands: a node's start,
    /// a close that `counts_close` admits, or the tree the latest wake found,
    /// whichever came last. No node records a change a person made to the
    /// tree; the wake that finds it is the first record of that change, so
    /// the wake counts. `None` when no record names a tree.
    pub fn newest_tree(&self, counts_close: impl Fn(&NodeId) -> bool) -> Option<(Seq, &TreeId)> {
        self.nodes
            .latest_tree(counts_close)
            .into_iter()
            .chain(self.run.woke_on())
            .max_by_key(|(seq, _)| *seq)
    }

    /// Whether the run's tree moved since `node`'s latest failure left it.
    /// `false` for a node whose failure named no tree.
    pub fn tree_moved_since_failure(&self, node: &NodeId) -> bool {
        let Some(failed) = self
            .nodes
            .get(node)
            .and_then(|record| record.failed_tree.as_ref())
        else {
            return false;
        };
        self.newest_tree(|_| true)
            .is_some_and(|(_, tree)| tree != failed)
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use yunta_core::events::{
        EventBody, EventPayload, Failure, NodeEvent, NodeFailedPayload, NodeFinishedPayload,
        NodeOutcome, NodeStartedPayload, RunEvent, RunResumedPayload, StoredEvent, TokenUsage,
    };
    use yunta_core::{NodeId, Seq, TreeId};

    use crate::replay::derive;

    /// The log `entries` make, in order, each under the node it concerns.
    fn log(entries: Vec<(Option<&str>, EventPayload)>) -> Vec<StoredEvent> {
        entries
            .into_iter()
            .enumerate()
            .map(|(index, (node, payload))| StoredEvent {
                run_id: "run-1".into(),
                seq: (index as u64 + 1).into(),
                timestamp: DateTime::UNIX_EPOCH,
                node_id: node.map(Into::into),
                body: EventBody::Known(payload),
            })
            .collect()
    }

    /// The first attempt of `node`, started on `tree`.
    fn started(node: &'static str, tree: &str) -> (Option<&'static str>, EventPayload) {
        let start = NodeStartedPayload::attempt_from(1, tree.parse().unwrap());
        (Some(node), EventPayload::Node(NodeEvent::Started(start)))
    }

    /// A failure of `node` that left the run's tree on `tree`.
    fn failed(node: &'static str, tree: &str) -> (Option<&'static str>, EventPayload) {
        let failure = NodeFailedPayload::new(
            Failure::Message {
                outcome: "regression".into(),
            },
            TokenUsage::default(),
        );
        let failure = failure.leaving(tree.parse().unwrap());
        (Some(node), EventPayload::Node(NodeEvent::Failed(failure)))
    }

    /// A close of `node` that left the run's tree on `tree`.
    fn finished(node: &'static str, tree: &str) -> (Option<&'static str>, EventPayload) {
        let outcome = NodeOutcome::Summary("ok".to_string());
        let close =
            NodeFinishedPayload::leaving(outcome, TokenUsage::default(), tree.parse().unwrap());
        (Some(node), EventPayload::Node(NodeEvent::Finished(close)))
    }

    /// A wake that found the run's tree on `tree`.
    fn wake_found(tree: &str) -> (Option<&'static str>, EventPayload) {
        let wake = RunResumedPayload::new(Vec::new(), None).finding(tree.parse().unwrap());
        (None, EventPayload::Run(RunEvent::Resumed(wake)))
    }

    /// Whether the tree moved since `node`'s failure, as `entries` record it.
    fn moved(entries: Vec<(Option<&'static str>, EventPayload)>, node: &str) -> bool {
        derive(&log(entries)).tree_moved_since_failure(&NodeId::from(node))
    }

    #[test]
    fn a_node_that_never_failed_reads_no_movement() {
        let entries = vec![started("compare", "a1a1a1a"), started("other", "b2b2b2b")];
        assert!(!moved(entries, "compare"));
    }

    #[test]
    fn a_failure_reads_no_movement_while_the_tree_stands_where_it_left_it() {
        let entries = vec![started("compare", "a1a1a1a"), failed("compare", "a1a1a1a")];
        assert!(!moved(entries, "compare"));
    }

    #[test]
    fn a_later_close_that_leaves_the_tree_as_the_failure_did_is_no_movement() {
        let entries = vec![
            started("compare", "a1a1a1a"),
            failed("compare", "a1a1a1a"),
            started("other", "a1a1a1a"),
            finished("other", "a1a1a1a"),
        ];
        assert!(!moved(entries, "compare"));
    }

    #[test]
    fn a_later_close_that_moves_the_tree_is_movement_since_the_failure() {
        let entries = vec![
            started("compare", "a1a1a1a"),
            failed("compare", "a1a1a1a"),
            started("other", "a1a1a1a"),
            finished("other", "b2b2b2b"),
        ];
        assert!(moved(entries, "compare"));
    }

    #[test]
    fn a_wake_that_found_the_tree_moved_is_movement_though_no_node_moved_it() {
        let entries = vec![
            started("compare", "a1a1a1a"),
            failed("compare", "a1a1a1a"),
            wake_found("b2b2b2b"),
        ];
        assert!(moved(entries, "compare"));
    }

    #[test]
    fn a_wake_that_found_the_tree_where_the_failure_left_it_is_no_movement() {
        let entries = vec![
            started("compare", "a1a1a1a"),
            failed("compare", "a1a1a1a"),
            wake_found("a1a1a1a"),
        ];
        assert!(!moved(entries, "compare"));
    }

    #[test]
    fn a_close_after_a_wake_is_the_newer_record_of_the_tree() {
        let entries = vec![
            started("compare", "a1a1a1a"),
            failed("compare", "a1a1a1a"),
            wake_found("b2b2b2b"),
            started("other", "b2b2b2b"),
            finished("other", "a1a1a1a"),
        ];
        assert!(
            !moved(entries, "compare"),
            "the close put the tree back where the failure left it"
        );
    }

    #[test]
    fn a_failure_that_named_no_tree_reads_no_movement() {
        let unnamed = NodeFailedPayload::new(
            Failure::Message {
                outcome: "regression".into(),
            },
            TokenUsage::default(),
        );
        let entries = vec![
            started("compare", "a1a1a1a"),
            (
                Some("compare"),
                EventPayload::Node(NodeEvent::Failed(unnamed)),
            ),
            wake_found("b2b2b2b"),
        ];
        assert!(!moved(entries, "compare"));
    }

    #[test]
    fn a_close_the_predicate_refuses_is_no_newest_record_but_a_wake_is() {
        let entries = || {
            vec![
                started("compare", "a1a1a1a"),
                failed("compare", "a1a1a1a"),
                started("other", "a1a1a1a"),
                finished("other", "b2b2b2b"),
            ]
        };
        let counted = |node: &NodeId| node.as_str() != "other";
        let state = derive(&log(entries()));
        let a1: TreeId = "a1a1a1a".parse().unwrap();
        let b2: TreeId = "b2b2b2b".parse().unwrap();
        assert_eq!(state.newest_tree(counted), Some((Seq::from(3), &a1)));
        assert_eq!(state.newest_tree(|_| true), Some((Seq::from(4), &b2)));

        let mut found = entries();
        found.push(wake_found("c3c3c3c"));
        let state = derive(&log(found));
        let c3: TreeId = "c3c3c3c".parse().unwrap();
        assert_eq!(state.newest_tree(counted), Some((Seq::from(5), &c3)));
    }
}
