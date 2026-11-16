//! The chronicle reads a log as replay reads it: the chronicle of a prefix
//! is a prefix of the chronicle, the state a moment says a node reached is
//! the state the log derives at that event, and a log chronicled in any
//! chunks, some of its events handed twice, past a break too, tells its
//! chronicle with one moment for every event.

use super::*;

/// `log` chronicled into a fresh state chunk by chunk, cut at each of
/// `cuts`: the state it leaves, and the moments its chunks earned, joined.
fn chronicled_in_chunks(log: &[StoredEvent], cuts: &[usize]) -> (RunState, Vec<Moment>) {
    let mut state = RunState::default();
    let mut moments = Vec::new();
    for chunk in chunks(log, cuts) {
        moments.extend(chunk.iter().filter_map(|event| state.chronicled(event)));
    }
    (state, moments)
}

proptest! {
    /// The chronicle of a prefix is a prefix of the chronicle: a reader
    /// following a run live sees exactly what a reader of the finished
    /// log sees, in the same order, and nothing a later event reveals
    /// rewrites a moment already read.
    #[test]
    fn the_chronicle_of_a_prefix_is_a_prefix_of_the_chronicle(log in log()) {
        let whole = chronicle(&log);
        prop_assert_eq!(whole.len(), log.len(), "one moment per event");
        for k in 0..=log.len() {
            prop_assert_eq!(
                chronicle(&log[..k]),
                whole[..k].to_vec(),
                "the chronicle of the first {} events is not its prefix",
                k
            );
        }
    }

    /// The frame agrees with the chronicle: the state a moment says a
    /// node reached is the state the log derives at that very event.
    ///
    /// Two derivations of one log that disagreed would let a region and
    /// a scrollback describe the same node two ways, which is the
    /// defect this pair exists to make impossible. Stated per moment
    /// rather than per node, because that is what makes it true of a
    /// node read halfway through the log as well as at its end.
    #[test]
    fn the_frame_agrees_with_the_chronicle(log in log()) {
        for (index, moment) in chronicle(&log).into_iter().enumerate() {
            let (Some(node), Happening::Node(NodeHappening::Reached { state, .. })) =
                (moment.node, moment.happening)
            else {
                continue;
            };
            let at_that_event = RunState::replay(&log[..=index]);
            if at_that_event.broken.is_some() {
                // A log the replay could not fold that far derives
                // nothing past its break, and the chronicle stops there
                // with it: there is no state for the two to agree on.
                continue;
            }
            prop_assert_eq!(
                at_that_event.nodes.state(&node),
                Some(&state),
                "a node is where its own moment says it became, at that moment: {}",
                node
            );
        }
    }

    /// A reader that chronicles a run's log as it comes, a few events at a
    /// time, tells what a reader of the whole log tells, moment for moment,
    /// and holds the state its replay derives: where the reads were cut
    /// changes nothing.
    #[test]
    fn chronicling_a_log_in_any_chunks_equals_its_chronicle(
        (log, cuts) in log().prop_flat_map(cut)
    ) {
        let (state, moments) = chronicled_in_chunks(&log, &cuts);
        prop_assert_eq!(moments, chronicle(&log));
        prop_assert_eq!(&state, &RunState::replay(&log));
        prop_assert_eq!(state.head(), log.last().map(|event| event.seq));
    }

    /// Events handed again, as a reader handed the same news twice, earn no
    /// second moment: a log read up to a point, then read again from an
    /// earlier one to its end, tells its chronicle once.
    #[test]
    fn an_event_handed_twice_earns_one_moment(
        (log, read, again) in log()
            .prop_flat_map(|log| {
                let len = log.len();
                (Just(log), 0..=len)
            })
            .prop_flat_map(|(log, read)| (Just(log), Just(read), 0..=read))
    ) {
        let mut state = RunState::default();
        let mut moments: Vec<Moment> =
            log[..read].iter().filter_map(|event| state.chronicled(event)).collect();
        moments.extend(log[again..].iter().filter_map(|event| state.chronicled(event)));
        prop_assert_eq!(moments, chronicle(&log));
        prop_assert_eq!(&state, &RunState::replay(&log));
    }

    /// Past the event where a log broke, each event still earns its moment
    /// and moves nothing else: a reader is told the log carries more than
    /// could be folded, in whatever chunks it reads it.
    #[test]
    fn the_chronicle_of_a_broken_log_has_a_moment_for_every_event(
        (log, cuts) in a_broken_log().prop_flat_map(cut)
    ) {
        let (state, moments) = chronicled_in_chunks(&log, &cuts);
        prop_assert!(state.broken.is_some(), "the event that names no node breaks the log");
        prop_assert_eq!(
            moments.iter().map(|moment| moment.seq).collect::<Vec<_>>(),
            log.iter().map(|event| event.seq).collect::<Vec<_>>(),
            "one moment for every event, in the log's order"
        );
        prop_assert_eq!(moments, chronicle(&log));
        prop_assert_eq!(&state, &RunState::replay(&log));
    }
}
