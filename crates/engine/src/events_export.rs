//! `events.jsonl` export — at close, the engine writes
//! the run's whole event log to `run.dir` as JSON Lines: the archived run
//! is then self-contained, readable years later independent of
//! `storage.retention_days` deleting the DB's own copy.
//!
//! One JSON object per line, in `seq` order — the exact same [`Event`]
//! shape the DB stores (`EventPayload`'s own tag carries `kind`; no
//! separate envelope). This is deliberately the closest thing to "the
//! bytes as stored": an exported JSONL is treated as unmigratable once
//! it leaves the system, so read-side normalization (not a rewritten
//! export) is what has to carry compatibility for it — this module
//! only ever writes the current in-memory shape.

use yunta_core::events::Event;

#[derive(Debug, thiserror::Error)]
pub enum EventsExportError {
    #[error("failed to serialize event at seq {seq} for events.jsonl: {detail}")]
    Serialize { seq: u64, detail: String },
}

/// Renders `events`, in the order given, as JSON Lines — pure, no IO. The
/// caller is responsible for the order being `seq`-ascending (every
/// caller in this codebase reads events that way already).
pub fn render_events_jsonl(events: &[Event]) -> Result<String, EventsExportError> {
    let mut out = String::new();
    for event in events {
        let line = serde_json::to_string(event).map_err(|e| EventsExportError::Serialize {
            seq: event.seq,
            detail: e.to_string(),
        })?;
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}
