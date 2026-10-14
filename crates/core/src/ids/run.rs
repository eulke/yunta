//! A run's id: the name of its directory under the runs root, and the short
//! handle a person calls the run by.

use std::borrow::{Borrow, Cow};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{is_segment, string_id, InvalidId, SEGMENT_RULE};

string_id!(
    /// A run's id — the name of its directory under the runs root.
    RunId, what = "run id", rule = SEGMENT_RULE, check = is_segment
);

/// A ULID is 26 Crockford base32 characters — one path segment by
/// construction, so the conversion is total.
impl From<ulid::Ulid> for RunId {
    fn from(ulid: ulid::Ulid) -> Self {
        RunId(Cow::Owned(ulid.to_string()))
    }
}

impl RunId {
    /// How many characters of a run's id its handle keeps: enough that
    /// two runs on one machine sharing one is a collision nobody meets,
    /// few enough to read off one line and type into the next.
    pub const HANDLE_CHARS: usize = 6;

    /// What a person calls the run by: the last [`RunId::HANDLE_CHARS`]
    /// characters of its id. A ULID opens with the time it was made,
    /// which every run of a day shares, and closes with its random part,
    /// which tells two runs apart even when they were made in the same
    /// millisecond. An id no longer than a handle is its own.
    pub fn handle(&self) -> &str {
        let id = self.as_str();
        let start = id
            .char_indices()
            .rev()
            .nth(Self::HANDLE_CHARS - 1)
            .map_or(0, |(at, _)| at);
        id.get(start..).unwrap_or(id)
    }
}
