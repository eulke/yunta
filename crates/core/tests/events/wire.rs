//! Every event survives a trip through its JSON wire form unchanged, over
//! generated logs.

use proptest::prelude::*;
use yunta_core::events::StoredEvent;
use yunta_testkit_core::arbitrary::log;

proptest! {
    /// Every event survives a trip through its JSON wire form unchanged —
    /// the persistence and `events.jsonl` contract.
    #[test]
    fn events_round_trip_through_json(log in log()) {
        for event in &log {
            let json = serde_json::to_string(event).expect("serialize");
            let back: StoredEvent = serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(&back, event);
        }
    }
}
