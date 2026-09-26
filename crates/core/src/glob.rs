//! The one compilation rule for a scope glob — a node's `scope`, a
//! task's `scope`, an expansion's `within`. `*` never crosses a `/`;
//! `**` is how a pattern asks for recursion. A scope is a ceiling on
//! what may change, and a ceiling reads with the strictest
//! interpretation its syntax allows.

use std::fmt;
use std::str::FromStr;

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One scope pattern, compiled. A `ScopeGlob` exists only for a pattern
/// globset can evaluate, so every consumer of a scope matches against
/// it instead of discovering at match time that the ceiling is
/// unreadable.
#[derive(Clone)]
pub struct ScopeGlob(Glob);

impl ScopeGlob {
    /// The pattern as authored — what a diagnostic prints and what a
    /// serialized scope carries.
    pub fn as_str(&self) -> &str {
        self.0.glob()
    }

    /// The compiled glob, for a caller building its own set.
    pub fn compiled(&self) -> &Glob {
        &self.0
    }
}

/// What a pattern that does not compile is refused with.
#[derive(Debug, thiserror::Error)]
#[error("`{pattern}` is not a scope glob")]
pub struct InvalidScopeGlob {
    pub pattern: String,
    #[source]
    pub source: globset::Error,
}

impl FromStr for ScopeGlob {
    type Err = InvalidScopeGlob;

    fn from_str(pattern: &str) -> Result<Self, InvalidScopeGlob> {
        scope_glob(pattern)
            .map(ScopeGlob)
            .map_err(|source| InvalidScopeGlob {
                pattern: pattern.to_string(),
                source,
            })
    }
}

impl TryFrom<String> for ScopeGlob {
    type Error = InvalidScopeGlob;

    fn try_from(pattern: String) -> Result<Self, InvalidScopeGlob> {
        pattern.parse()
    }
}

impl fmt::Debug for ScopeGlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ScopeGlob").field(&self.as_str()).finish()
    }
}

impl fmt::Display for ScopeGlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq for ScopeGlob {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for ScopeGlob {}

impl PartialOrd for ScopeGlob {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScopeGlob {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl std::hash::Hash for ScopeGlob {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl PartialEq<str> for ScopeGlob {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for ScopeGlob {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl Serialize for ScopeGlob {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ScopeGlob {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let pattern = String::deserialize(deserializer)?;
        pattern.parse().map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for ScopeGlob {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ScopeGlob".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 1,
            "description": "a scope glob: a path pattern where `*` stays within one directory and `**` recurses",
        })
    }
}

/// Test convenience: a literal that does not compile panics with the
/// reason. Production code parses instead.
#[cfg(any(test, feature = "testkit"))]
// A literal that does not parse is a test-authoring mistake, meant to
// abort the test loudly — the one place a panic is the right answer.
#[allow(clippy::panic)]
impl From<&str> for ScopeGlob {
    fn from(pattern: &str) -> Self {
        match pattern.parse() {
            Ok(glob) => glob,
            Err(error) => panic!("{}", crate::describe(&error)),
        }
    }
}

/// Compiles one scope pattern.
fn scope_glob(pattern: &str) -> Result<Glob, globset::Error> {
    GlobBuilder::new(pattern).literal_separator(true).build()
}

/// Compiles a whole scope. Every pattern already compiled on its own,
/// so the only failure left is the set's own size limit.
pub fn scope_globset(patterns: &[ScopeGlob]) -> Result<GlobSet, globset::Error> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(pattern.compiled().clone());
    }
    builder.build()
}

/// A scope run together for one sentence: the patterns as authored,
/// comma-separated. The one place a list of globs becomes prose.
pub fn listed_globs(globs: &[ScopeGlob]) -> String {
    globs
        .iter()
        .map(ScopeGlob::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether two globs might select the same file.
///
/// A deliberately conservative approximation, not full glob algebra:
/// two globs are related when one's literal prefix — everything before
/// its first `*`, `?` or `[` — starts with the other's. It can relate a
/// pair that would not actually collide (`src/*.rs` does not recurse
/// into `src/sub/`) and never misses one that would. For a check whose
/// job is to stop two tasks editing the same file, a false alarm an
/// author adjusts is the right side to err on; a silent miss is not.
///
/// One heuristic with two callers — the tasks document's own scope rule and the
/// workflow's `parallel` scope-collision check — rather than two copies
/// drifting apart.
pub fn might_overlap(a: &ScopeGlob, b: &ScopeGlob) -> bool {
    let (pa, pb) = (literal_prefix(a.as_str()), literal_prefix(b.as_str()));
    pa.starts_with(pb) || pb.starts_with(pa)
}

/// Everything before the first wildcard: the part of a glob that is a
/// plain path.
fn literal_prefix(glob: &str) -> &str {
    let end = glob.find(['*', '?', '[']).unwrap_or(glob.len());
    glob.get(..end).unwrap_or(glob)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_star_never_crosses_a_directory_and_a_double_star_does() {
        let set = scope_globset(&["src/*.rs".into()]).unwrap();
        assert!(set.is_match("src/lib.rs"));
        assert!(!set.is_match("src/deep/nested.rs"));
        let set = scope_globset(&["src/**".into()]).unwrap();
        assert!(set.is_match("src/deep/nested.rs"));
    }

    #[test]
    fn a_pattern_that_does_not_compile_has_no_scope_glob() {
        let error = "src/[unclosed".parse::<ScopeGlob>().unwrap_err();
        assert_eq!(error.pattern, "src/[unclosed");
    }
}
