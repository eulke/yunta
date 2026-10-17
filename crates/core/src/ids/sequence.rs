//! The sequence number of an event in its run's log: a positive position
//! that storage assigns, never the engine.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::InvalidId;

/// An event's position in its run's log: 1 for the run's first event,
/// counting up without gaps. Assigned by storage when a draft is
/// appended; never chosen by the engine.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Seq(u64);

const SEQ_RULE: &str = "a positive number";

impl Seq {
    /// The first event of every run.
    pub const FIRST: Seq = Seq(1);

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The position right after this one.
    pub const fn next(self) -> Seq {
        Seq(self.0 + 1)
    }
}

impl fmt::Debug for Seq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Seq({})", self.0)
    }
}

impl fmt::Display for Seq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<i64> for Seq {
    type Error = InvalidId;

    fn try_from(value: i64) -> Result<Self, InvalidId> {
        u64::try_from(value)
            .ok()
            .filter(|raw| *raw > 0)
            .map(Seq)
            .ok_or_else(|| InvalidId {
                what: "seq",
                value: value.to_string(),
                rule: SEQ_RULE,
            })
    }
}

impl Serialize for Seq {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for Seq {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u64::deserialize(deserializer)?;
        i64::try_from(value)
            .map_err(|_| InvalidId {
                what: "seq",
                value: value.to_string(),
                rule: SEQ_RULE,
            })
            .and_then(Seq::try_from)
            .map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Seq {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Seq".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "integer",
            "minimum": 1,
            "description": format!("A position in a run's log: {}", SEQ_RULE),
        })
    }
}

/// Test convenience: a literal that is not a position panics. Production
/// code takes the seq storage assigned.
#[cfg(any(test, feature = "testkit"))]
// A bad literal in a fixture is a test-authoring mistake, meant to abort
// the test loudly — the one place a panic is the right answer.
#[allow(clippy::panic)]
impl From<u64> for Seq {
    fn from(value: u64) -> Self {
        match i64::try_from(value).ok().map(Seq::try_from) {
            Some(Ok(seq)) => seq,
            _ => panic!("`{value}` is not a valid seq: {SEQ_RULE}"),
        }
    }
}
