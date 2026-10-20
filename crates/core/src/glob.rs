//! The one compilation rule for a scope glob — a node's `scope`, a
//! task's `scope`, a `permissions.paths.deny` entry. `*` never crosses a
//! `/`; `**` is how a pattern asks for recursion. A scope is a ceiling on
//! what may change, and a ceiling reads with the strictest
//! interpretation its syntax allows.

use std::fmt;
use std::path::{Path, PathBuf};
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

    /// The pattern that selects exactly `path`: every character globset
    /// reads as syntax is escaped, so a file named `a[1].rs` is that file
    /// rather than a character class, and `{draft}.md` is not `draft.md`.
    pub fn exact(path: &Path) -> Result<Self, InvalidScopeGlob> {
        literal_text(path).parse()
    }

    /// The path an [`ScopeGlob::exact`] glob selects, read back from its
    /// escapes; `None` for a glob that can select more than one path.
    pub fn exact_path(&self) -> Option<PathBuf> {
        let mut path = String::new();
        let mut chars = self.as_str().chars();
        while let Some(c) = chars.next() {
            match c {
                '[' => {
                    let inner = chars.next()?;
                    (chars.next()? == ']').then_some(())?;
                    path.push(inner);
                }
                '\\' => path.push(chars.next()?),
                '*' | '?' | ']' | '{' | '}' => return None,
                c => path.push(c),
            }
        }
        Some(PathBuf::from(path))
    }

    /// The pattern that selects every path under the directory `path`,
    /// escaped as [`ScopeGlob::exact`] escapes it. A sandbox denies it as a
    /// tree only when `path` holds no character that needs escaping, since
    /// [`ScopeGlob::whole_tree`] reads no escape.
    pub fn under(path: &Path) -> Result<Self, InvalidScopeGlob> {
        format!("{}/**", literal_text(path)).parse()
    }

    /// The directory every path the glob selects lies under: its text
    /// before the first wildcard, cut at the last `/`. `.codex/**` gives
    /// `.codex` and `src/a*.rs` gives `src`; `**/*.md` and `.mcp.json` name
    /// no directory and give `None`. This answers the git pathspecs, which
    /// look through that directory. It is not a deny: `src` holds paths
    /// `src/a*.rs` does not select, so a sandbox denies
    /// [`ScopeGlob::whole_tree`] instead.
    pub fn tree(&self) -> Option<&str> {
        let before = prefix(self);
        let cut = before.rfind('/')?;
        before.get(..cut).filter(|tree| !tree.is_empty())
    }

    /// The directory the glob selects every path of, when it selects every
    /// path under one: its text is `dir/**` or `dir/**/*` with `dir` free of
    /// glob syntax. `.codex/**` gives `.codex` and `src/generated/**/*` gives
    /// `src/generated`; `src/a*.rs`, `config/secrets.yaml`, `**/AGENTS.md`,
    /// `src/*/gen/**` and `**` give `None`: each selects a file, part of a
    /// directory, or paths under no one directory.
    pub fn whole_tree(&self) -> Option<&str> {
        let text = self.as_str();
        let dir = text
            .strip_suffix("/**/*")
            .or_else(|| text.strip_suffix("/**"))?;
        (!dir.is_empty() && !dir.contains(['*', '?', '[', '{', '\\'])).then_some(dir)
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

/// `path` as glob text that selects only itself: each character globset reads
/// as syntax stands in a class of its own, and a backslash is escaped.
fn literal_text(path: &Path) -> String {
    let mut text = String::new();
    for c in path.to_string_lossy().chars() {
        match c {
            '?' | '*' | '[' | ']' | '{' | '}' => {
                text.push('[');
                text.push(c);
                text.push(']');
            }
            '\\' => text.push_str("\\\\"),
            c => text.push(c),
        }
    }
    text
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

/// Whether `glob` might match a path one of `denied` matches — how a
/// request or a grant is refused before anything is written.
pub fn reaches_any(glob: &ScopeGlob, denied: &[ScopeGlob]) -> bool {
    denied.iter().any(|deny| might_overlap(glob, deny))
}

/// Whether two globs might select the same file. It may relate a pair that
/// selects no common file (`src/*.rs` and `src/sub/**`), and it never
/// misses a pair that does.
///
/// A glob with no wildcard selects exactly its own path, so it is tested
/// with the other glob's matcher. Two wildcard globs relate unless their
/// literal prefixes (the text before the first `*`, `?`, `[` or `{`)
/// contradict each other, or their literal suffixes (the text after the
/// last `*`) do. A glob with a backslash escape is never read as literal
/// text and promises no prefix or suffix, so it relates to any wildcard
/// glob.
///
/// One overlap rule for every caller: the tasks document's scope rule, the
/// workflow's `parallel` scope-collision check, and what a protected path
/// reaches.
pub fn might_overlap(a: &ScopeGlob, b: &ScopeGlob) -> bool {
    match (literal(a), literal(b)) {
        (Some(path), _) => matches_path(b, path),
        (_, Some(path)) => matches_path(a, path),
        (None, None) => {
            let (pa, pb, sa, sb) = (prefix(a), prefix(b), suffix(a), suffix(b));
            (pa.starts_with(pb) || pb.starts_with(pa)) && (sa.ends_with(sb) || sb.ends_with(sa))
        }
    }
}

/// Whether `glob` selects `path`.
fn matches_path(glob: &ScopeGlob, path: &str) -> bool {
    glob.compiled().compile_matcher().is_match(path)
}

/// A glob with no wildcard and no escape selects exactly its own text.
fn literal(glob: &ScopeGlob) -> Option<&str> {
    let text = glob.as_str();
    (!text.contains(['*', '?', '[', '{', '\\'])).then_some(text)
}

/// The text before the first wildcard: every path the glob can select
/// starts with it. An escape spells a character the text does not show,
/// so a glob with one promises no prefix.
fn prefix(glob: &ScopeGlob) -> &str {
    let text = glob.as_str();
    if text.contains('\\') {
        return "";
    }
    let end = text.find(['*', '?', '[', '{']).unwrap_or(text.len());
    text.get(..end).unwrap_or_default()
}

/// The text after the last `*`: every path the glob can select ends with
/// it. Empty when a class or an alternation closes after that `*` (the
/// star may sit inside it, so the text after it is no suffix), or when an
/// escape makes the text unreadable.
fn suffix(glob: &ScopeGlob) -> &str {
    let text = glob.as_str();
    if text.contains('\\') {
        return "";
    }
    let start = text.rfind('*').map_or(0, |index| index + 1);
    let after = text.get(start..).unwrap_or_default();
    if after.contains(['?', '[', ']', '{', '}']) {
        ""
    } else {
        after
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_glob_selects_its_path_and_nothing_its_characters_could_spell() {
        let exact = ScopeGlob::exact(Path::new("src/a[1].rs")).unwrap();
        let set = scope_globset(&[exact]).unwrap();
        assert!(set.is_match("src/a[1].rs"));
        assert!(!set.is_match("src/a1.rs"));
        let named = Path::new("docs/{draft}\\x.md");
        let exact = ScopeGlob::exact(named).unwrap();
        assert_eq!(exact.exact_path().as_deref(), Some(named));
        assert_eq!(ScopeGlob::from("src/*.rs").exact_path(), None);
        let set = scope_globset(&[exact]).unwrap();
        assert!(set.is_match("docs/{draft}\\x.md"));
        assert!(!set.is_match("docs/draftx.md"));
        assert!(!set.is_match("docs/draft\\x.md"));
    }

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

    #[test]
    fn a_wildcard_deny_reaches_only_the_paths_it_can_select() {
        let lock = [ScopeGlob::from("**/*.lock")];
        assert!(!reaches_any(&ScopeGlob::from("src/a.rs"), &lock));
        assert!(reaches_any(&ScopeGlob::from("vendor/x.lock"), &lock));
        assert!(!reaches_any(&ScopeGlob::from("**/*_test.rs"), &lock));
    }

    #[test]
    fn two_wildcards_relate_only_when_their_prefixes_and_suffixes_agree() {
        assert!(might_overlap(
            &ScopeGlob::from("src/**"),
            &ScopeGlob::from("src/*.rs")
        ));
        assert!(!might_overlap(
            &ScopeGlob::from("src/*.rs"),
            &ScopeGlob::from("docs/*.md")
        ));
        assert!(!might_overlap(
            &ScopeGlob::from("*.rs"),
            &ScopeGlob::from("*.md")
        ));
    }

    #[test]
    fn a_literal_is_tested_with_the_other_glob_as_a_matcher() {
        assert!(might_overlap(
            &ScopeGlob::from("src/lib.rs"),
            &ScopeGlob::from("src/**")
        ));
        assert!(!might_overlap(
            &ScopeGlob::from("src/lib.rs"),
            &ScopeGlob::from("docs/**")
        ));
    }

    #[test]
    fn an_escape_is_read_as_the_character_it_spells() {
        // On unix `\b` is `b`, so `a\b*` selects `ab` followed by anything.
        assert!(might_overlap(
            &ScopeGlob::from("a\\b*"),
            &ScopeGlob::from("ab*")
        ));
    }

    #[test]
    fn a_star_inside_an_alternation_promises_no_suffix() {
        // `a{*,b}` selects `axz`, which `*z` selects too.
        assert!(might_overlap(
            &ScopeGlob::from("a{*,b}"),
            &ScopeGlob::from("*z")
        ));
    }

    #[test]
    fn an_escape_promises_no_prefix_so_its_pattern_may_overlap() {
        // `\*` is a literal star, which the text before the wildcard does not
        // promise: the pattern is not taken for a literal, so it may overlap.
        assert!(might_overlap(
            &ScopeGlob::from("x\\*"),
            &ScopeGlob::from("y/*")
        ));
    }

    #[test]
    fn a_glob_names_the_directory_before_its_first_wildcard() {
        let tree = |glob: &str| ScopeGlob::from(glob).tree().map(str::to_string);
        assert_eq!(tree(".codex/**").as_deref(), Some(".codex"));
        assert_eq!(tree("src/a*.rs").as_deref(), Some("src"));
        assert_eq!(tree("config/secrets.yaml").as_deref(), Some("config"));
        assert_eq!(tree("**/AGENTS.md"), None);
        assert_eq!(tree(".mcp.json"), None);
        assert_eq!(tree("*.lock"), None);
    }

    #[test]
    fn only_a_directory_then_a_double_star_selects_a_whole_tree() {
        let whole = |glob: &str| ScopeGlob::from(glob).whole_tree().map(str::to_string);
        assert_eq!(whole(".codex/**").as_deref(), Some(".codex"));
        assert_eq!(whole(".git/**").as_deref(), Some(".git"));
        assert_eq!(
            whole("src/generated/**/*").as_deref(),
            Some("src/generated")
        );
        assert_eq!(whole("src/a*.rs"), None);
        assert_eq!(whole("config/secrets.yaml"), None);
        assert_eq!(whole(".mcp.json"), None);
        assert_eq!(whole("**/AGENTS.md"), None);
        assert_eq!(whole("*.lock"), None);
        assert_eq!(whole("src/*/gen/**"), None);
        assert_eq!(whole("**"), None);
    }
}
