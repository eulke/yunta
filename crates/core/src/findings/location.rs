//! Where a finding is: what it is relative to, the file it concerns,
//! and the lines of it when the finder knew them.
//!
//! A location reaches the engine as one string — `src/lib.rs`,
//! `src/lib.rs:10-14`, `run:scratch/engine.json` — because that is what
//! an agent types and what an editor jumps to. It is read into its
//! parts where it arrives, so a surface that wants the file alone never
//! re-splits the string.
//!
//! Never an absolute path. A findings artifact is inherited by a
//! successor run that may not be on this host, so `/home/…/objects` is
//! not a fact about anything — it is this machine's accident. What a
//! location says instead is which of the run's two roots it is under:
//! the work, or the run's own register (D175).

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// A finding's place: what it is relative to, the path under that root,
/// and the lines of it the finder named.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    pub root: LocationRoot,
    pub path: RelativePath,
    pub range: Option<LineRange>,
}

/// What a location is relative to. The run has two places and a finding
/// is about one of them: what the agents change, or what the engine
/// keeps about the changing. The same two the templates name
/// (`{{run.worktree}}`, `{{run.dir}}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum LocationRoot {
    /// The run's worktree — the project. What an agent's finding is
    /// always about, so it is what a location with no prefix means.
    #[default]
    Work,
    /// The run's own directory: the register it keeps about the work —
    /// the object store, the process registry, a declared artifact.
    /// Only the engine's own findings are about it.
    Run,
}

impl LocationRoot {
    /// The prefix a location carries to name this root, empty for the
    /// one a bare path means. The one spelling: [`Location`]'s `Display`
    /// writes it and its `FromStr` reads it.
    pub const fn prefix(self) -> &'static str {
        match self {
            LocationRoot::Work => "",
            LocationRoot::Run => "run:",
        }
    }

    /// Every root, in the order a reader meets them.
    pub const ALL: [LocationRoot; 2] = [LocationRoot::Work, LocationRoot::Run];
}

/// A path inside its root: relative, and never climbing out of it. A
/// finding that points outside the run points at something no reader of
/// this run can open.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativePath(PathBuf);

/// The lines a finding names: one, or a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    #[error(
        "`{path}` leaves the run: a location is relative to the work or to `run:`, and never \
         climbs out of it — an absolute path is this host's, not this run's"
    )]
    Escapes { path: String },
    #[error("`{text}` is not a line range: `10` or `10-14`")]
    NotARange { text: String },
    #[error("`{start}-{end}` ends before it starts")]
    Backwards { start: u32, end: u32 },
}

impl Location {
    /// A place in the work, which is what every finding an agent writes
    /// is about.
    pub fn work(path: RelativePath, range: Option<LineRange>) -> Self {
        Location {
            root: LocationRoot::Work,
            path,
            range,
        }
    }

    /// A place in the run's own register, which is what the engine's own
    /// findings are about.
    pub fn run(path: RelativePath, range: Option<LineRange>) -> Self {
        Location {
            root: LocationRoot::Run,
            path,
            range,
        }
    }

    /// The path alone, for a reader that only needs the file. Relative
    /// to [`root`](Self::root) — resolving it against a directory is the
    /// caller's, because only the caller knows where this run's two
    /// roots are on its own disk.
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl RelativePath {
    /// The root itself, which is what a finding about everything under
    /// it names.
    pub fn here() -> Self {
        RelativePath(PathBuf::from("."))
    }

    /// The path `parts` name together, keeping the names in them and
    /// nothing else.
    ///
    /// How a place the engine composes from its own constants and its
    /// own validated ids becomes a path: a separator, a `.` and a `..`
    /// name nothing, so what is built this way is relative and inside
    /// its root whatever the parts were, and naming a place the engine
    /// owns needs no conversion that can fail. Parts that name nothing
    /// at all give [`here`](Self::here).
    ///
    /// Authored text goes the other way: it parses, and what does not
    /// read is reported as [`InvalidLocation`] rather than repaired.
    pub fn of<P: AsRef<Path>>(parts: impl IntoIterator<Item = P>) -> Self {
        let mut path = PathBuf::new();
        for part in parts {
            for component in part.as_ref().components() {
                if let Component::Normal(name) = component {
                    path.push(name);
                }
            }
        }
        if path.as_os_str().is_empty() {
            RelativePath::here()
        } else {
            RelativePath(path)
        }
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        self.0.to_string_lossy()
    }
}

impl FromStr for Location {
    type Err = InvalidLocation;

    /// `path`, `path:10`, `path:10-14`, and any of those under a root's
    /// prefix (`run:path:10`). The last `:` separates the range, and
    /// only when what follows it reads as one: a path whose own name
    /// carries a colon keeps it.
    fn from_str(text: &str) -> Result<Self, InvalidLocation> {
        let (root, text) = root_of(text);
        let (path, range) = match text.rsplit_once(':') {
            Some((path, rest))
                if !rest.is_empty() && rest.starts_with(|c: char| c.is_ascii_digit()) =>
            {
                (path, Some(rest.parse::<LineRange>()?))
            }
            _ => (text, None),
        };
        Ok(Location {
            root,
            path: path.parse()?,
            range,
        })
    }
}

/// The root a location names, and what is left once its prefix is off.
/// A bare path is the work, which is what every finding an agent writes
/// is about — the prefix exists for the root only the engine reaches.
fn root_of(text: &str) -> (LocationRoot, &str) {
    for root in LocationRoot::ALL {
        let prefix = root.prefix();
        if !prefix.is_empty() {
            if let Some(rest) = text.strip_prefix(prefix) {
                return (root, rest);
            }
        }
    }
    (LocationRoot::Work, text)
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
        write!(f, "{}{}", self.root.prefix(), self.path.as_str())?;
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
            "description": "where a finding is: a path relative to the work, with its lines when they are known (`src/lib.rs`, `src/lib.rs:10`, `src/lib.rs:10-14`), or one relative to the run's own directory under the `run:` prefix (`run:objects`). Never absolute",
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
        for text in [
            "src/lib.rs",
            "src/lib.rs:10",
            "src/lib.rs:10-14",
            "run:objects",
            "run:scratch/engine.json",
            "run:artifacts/plan/report.md:3",
        ] {
            let location: Location = text.parse().unwrap();
            assert_eq!(location.to_string(), text);
        }
    }

    #[test]
    fn a_bare_path_is_the_work_and_a_prefix_is_the_root_it_names() {
        assert_eq!(
            "src/lib.rs".parse::<Location>().unwrap().root,
            LocationRoot::Work
        );
        assert_eq!(
            "run:objects".parse::<Location>().unwrap().root,
            LocationRoot::Run
        );
        // The prefix is consumed, never left in the path.
        assert_eq!(
            "run:objects".parse::<Location>().unwrap().path(),
            Path::new("objects")
        );
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
    fn an_absolute_path_is_never_a_location_whatever_its_root() {
        for text in ["/etc/passwd", "run:/var/lib/yunta"] {
            assert!(
                matches!(
                    text.parse::<Location>(),
                    Err(InvalidLocation::Escapes { .. })
                ),
                "a finding that names {text} names this host, not this run"
            );
        }
    }

    #[test]
    fn a_path_the_engine_composes_stays_inside_its_root() {
        assert_eq!(
            RelativePath::of(["scratch", "engine.json"]).as_path(),
            Path::new("scratch/engine.json")
        );
        assert_eq!(
            RelativePath::of(["artifacts/plan", "sub/dir/report.md"]).as_path(),
            Path::new("artifacts/plan/sub/dir/report.md")
        );
        assert_eq!(
            RelativePath::of(["/etc", "../../passwd"]).as_path(),
            Path::new("etc/passwd"),
            "what names nothing under the root contributes nothing"
        );
    }

    #[test]
    fn parts_that_name_nothing_are_the_root_itself() {
        let nothing: [&str; 0] = [];
        assert_eq!(RelativePath::of(nothing), RelativePath::here());
        assert_eq!(RelativePath::of([".."]), RelativePath::here());
        assert_eq!(
            Location::work(RelativePath::here(), None).to_string(),
            ".",
            "a finding about the whole work names the work"
        );
    }

    #[test]
    fn a_colon_the_range_cannot_read_stays_in_the_path() {
        let location: Location = "weird:name.rs".parse().unwrap();
        assert_eq!(location.path(), Path::new("weird:name.rs"));
        assert_eq!(location.range, None);
    }
}
