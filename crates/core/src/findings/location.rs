//! Where a finding is: the file it concerns, and the lines of it when
//! the finder knew them.
//!
//! A location reaches the engine as one string — `src/lib.rs`,
//! `src/lib.rs:10`, `src/lib.rs:10-14` — because that is what an agent
//! types and what an editor jumps to. It is read into its parts where it
//! arrives, so a surface that wants the file alone never re-splits the
//! string, and a location that says nothing is refused as the document
//! rule it breaks.

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// A finding's place: the path, relative to the worktree, and the lines
/// of it the finder named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub path: RelativePath,
    pub range: Option<LineRange>,
}

/// A path inside the work: relative, and never climbing out of it. A
/// finding that points outside the worktree points at something this
/// run cannot show.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativePath(PathBuf);

/// The lines a finding names: one, or a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: u32,
    /// `None` for a single line.
    pub end: Option<u32>,
}

/// A location that does not read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvalidLocation {
    #[error("a location names a path")]
    Empty,
    #[error("`{path}` leaves the work: a location is relative and never climbs out of it")]
    Escapes { path: String },
    #[error("`{text}` is not a line range: `10` or `10-14`")]
    NotARange { text: String },
    #[error("`{start}-{end}` ends before it starts")]
    Backwards { start: u32, end: u32 },
}

impl Location {
    /// The path alone, for a reader that only needs the file.
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl RelativePath {
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        self.0.to_string_lossy()
    }
}

impl FromStr for Location {
    type Err = InvalidLocation;

    /// `path`, `path:10` or `path:10-14`. The last `:` separates the
    /// range, and only when what follows it reads as one: a path whose
    /// own name carries a colon keeps it.
    fn from_str(text: &str) -> Result<Self, InvalidLocation> {
        let (path, range) = match text.rsplit_once(':') {
            Some((path, rest))
                if !rest.is_empty() && rest.starts_with(|c: char| c.is_ascii_digit()) =>
            {
                (path, Some(rest.parse::<LineRange>()?))
            }
            _ => (text, None),
        };
        Ok(Location {
            path: path.parse()?,
            range,
        })
    }
}

impl FromStr for RelativePath {
    type Err = InvalidLocation;

    fn from_str(text: &str) -> Result<Self, InvalidLocation> {
        if text.trim().is_empty() {
            return Err(InvalidLocation::Empty);
        }
        let path = Path::new(text);
        let climbs = path
            .components()
            .any(|component| matches!(component, Component::ParentDir));
        if path.is_absolute() || climbs {
            return Err(InvalidLocation::Escapes {
                path: text.to_string(),
            });
        }
        Ok(RelativePath(path.to_path_buf()))
    }
}

impl FromStr for LineRange {
    type Err = InvalidLocation;

    fn from_str(text: &str) -> Result<Self, InvalidLocation> {
        let not_a_range = || InvalidLocation::NotARange {
            text: text.to_string(),
        };
        let (start, end) = match text.split_once('-') {
            Some((start, end)) => (start, Some(end)),
            None => (text, None),
        };
        let start: u32 = start.parse().map_err(|_| not_a_range())?;
        let end = match end {
            Some(end) => Some(end.parse::<u32>().map_err(|_| not_a_range())?),
            None => None,
        };
        if let Some(end) = end {
            if end < start {
                return Err(InvalidLocation::Backwards { start, end });
            }
        }
        Ok(LineRange { start, end })
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path.as_str())?;
        match &self.range {
            Some(range) => write!(f, ":{range}"),
            None => Ok(()),
        }
    }
}

impl fmt::Display for LineRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.end {
            Some(end) => write!(f, "{}-{end}", self.start),
            None => write!(f, "{}", self.start),
        }
    }
}

impl Serialize for Location {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Location {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Location {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Location".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 1,
            "description": "where a finding is: a path relative to the work, with its lines when they are known — `src/lib.rs`, `src/lib.rs:10`, `src/lib.rs:10-14`",
        })
    }
}

/// Test convenience: a literal that is not a location panics with the
/// reason. Production code parses instead.
#[cfg(any(test, feature = "testkit"))]
// A bad literal in a fixture is a test-authoring mistake, meant to abort
// the test loudly — the one place a panic is the right answer.
#[allow(clippy::panic)]
impl From<&str> for Location {
    fn from(text: &str) -> Self {
        match text.parse() {
            Ok(location) => location,
            Err(error) => panic!("{error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_location_round_trips_through_its_one_spelling() {
        for text in ["src/lib.rs", "src/lib.rs:10", "src/lib.rs:10-14"] {
            let location: Location = text.parse().unwrap();
            assert_eq!(location.to_string(), text);
        }
    }

    #[test]
    fn a_path_that_climbs_out_of_the_work_is_not_a_location() {
        assert!(matches!(
            "../etc/passwd".parse::<Location>(),
            Err(InvalidLocation::Escapes { .. })
        ));
        assert!(matches!(
            "/etc/passwd".parse::<Location>(),
            Err(InvalidLocation::Escapes { .. })
        ));
    }

    #[test]
    fn a_range_that_ends_before_it_starts_is_not_a_range() {
        assert_eq!(
            "src/lib.rs:14-10".parse::<Location>(),
            Err(InvalidLocation::Backwards { start: 14, end: 10 })
        );
    }

    #[test]
    fn a_colon_the_range_cannot_read_stays_in_the_path() {
        let location: Location = "weird:name.rs".parse().unwrap();
        assert_eq!(location.path(), Path::new("weird:name.rs"));
        assert_eq!(location.range, None);
    }
}
