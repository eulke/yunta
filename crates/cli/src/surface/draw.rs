//! Where the folded run goes, and the two layouts of one chronicle.
//!
//! A surface does not choose words — [`super::chronicle`] does — and it
//! does not choose what happened — the engine's own derivation does.
//! What a surface chooses is what to keep and where to put it, and
//! those are the two answers here.

use yunta_engine::Moment;

use super::chronicle;
use super::lines::Lines;
use super::region::Region;
use crate::render::Glyphs;

/// Where the folded run goes.
pub(super) enum Draw {
    Lines(Lines),
    Live(Box<Region>),
}

impl Draw {
    /// Puts one moment of the run on this surface.
    ///
    /// The two differ in what they keep, never in what they call it: an
    /// append-only reader has no region to have watched, so every
    /// moment is written; a watched terminal keeps above its region
    /// what closed something or asked something of a person, because
    /// the region already shows the rest while it is true.
    pub(super) fn record(&mut self, moment: &Moment, glyphs: Glyphs) {
        match self {
            Draw::Lines(lines) => lines.moment(moment, glyphs),
            Draw::Live(region) => {
                if chronicle::kept(&moment.happening) {
                    region.record(&chronicle::graduation(moment, glyphs));
                }
            }
        }
    }
}

#[cfg(test)]
mod chronicle_tests {
    use yunta_core::events::{
        EventBody, EventPayload, Failure, NodeEvent, StoredEvent, TokenUsage,
    };

    use super::super::chronicle;
    use crate::render::Glyphs;

    fn event(seq: u64, node: &str, payload: EventPayload) -> StoredEvent {
        StoredEvent {
            run_id: "01JQ0000000000000000000000".into(),
            seq: seq.into(),
            timestamp: chrono::DateTime::UNIX_EPOCH + chrono::Duration::seconds(seq as i64),
            node_id: Some(node.into()),
            body: EventBody::Known(payload),
        }
    }

    #[test]
    fn what_a_watched_terminal_keeps_above_its_region_is_what_the_append_only_surface_writes() {
        // One derivation, two layouts. A reader who followed a run on a
        // terminal and a reader who read the same run out of a pipe met
        // the same sentences: what the terminal kept is a subsequence
        // of what the pipe wrote, in the same order and word for word.
        let events = vec![
            event(
                1,
                "lint",
                EventPayload::Node(NodeEvent::Started(yunta_core::events::NodeStartedPayload {
                    attempt: 1,
                })),
            ),
            event(
                2,
                "lint",
                EventPayload::Node(NodeEvent::Failed(
                    yunta_core::events::NodeFailedPayload::new(
                        Failure::message("exit 1"),
                        true,
                        TokenUsage::default(),
                    ),
                )),
            ),
            event(
                3,
                "lint",
                EventPayload::Node(NodeEvent::ContextAssembled(
                    serde_json::from_value(serde_json::json!({
                        "sources": [],
                        "segment_hashes": {},
                    }))
                    .expect("a context that assembled from nothing"),
                )),
            ),
        ];

        let moments = yunta_engine::chronicle(&events);
        let written: Vec<String> = moments
            .iter()
            .map(|moment| chronicle::say(moment).text)
            .collect();
        let history: Vec<String> = moments
            .iter()
            .filter(|moment| chronicle::kept(&moment.happening))
            .flat_map(|moment| chronicle::graduation(moment, Glyphs::Ascii))
            .map(|row| {
                row.trim_start_matches(['x', '+', '>', '!', '?', ' '])
                    .to_string()
            })
            .collect();

        let mut rest = written.iter();
        for kept in &history {
            assert!(
                rest.any(|line| line == kept),
                "the terminal kept a sentence the pipe never wrote, or wrote it out of order: \
                 {kept:?} in {written:?}"
            );
        }
        assert!(
            history.len() < written.len(),
            "a terminal keeps less than a pipe writes: {history:?} vs {written:?}"
        );
    }
}
