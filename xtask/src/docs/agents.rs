//! The agent docs as the checker and the digest read them: in `AGENTS.md`, its
//! tables and its rules; in `.agents/`, the entries (a `### ` heading and its
//! fields), the rules (a bullet and its tags) and the crate table.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;

use super::repo::Repo;

/// The map every agent CLI loads.
pub(super) const MAP: &str = "AGENTS.md";

/// The agent docs, the whole of `.agents/` but a skills root.
pub(super) const AGENT_DOCS: [&str; 5] = [
    ".agents/README.md",
    ".agents/architecture.md",
    ".agents/choices.md",
    ".agents/open.md",
    ".agents/traps.md",
];

/// Where rules stand: each doc with the `## ` sections that hold them.
pub(super) const RULE_SETS: [(&str, &[&str]); 2] = [
    (MAP, &["Rules"]),
    (
        ".agents/architecture.md",
        &["Product rules", "Cross-cutting"],
    ),
];

/// The map and the agent docs, by path; a file that is not there is absent.
pub(super) struct Docs {
    texts: BTreeMap<&'static str, String>,
}

impl Docs {
    pub(super) fn read(repo: &Repo) -> Result<Docs, String> {
        let mut texts = BTreeMap::new();
        for path in std::iter::once(MAP).chain(AGENT_DOCS) {
            if let Some(text) = repo.read_if_there(path) {
                texts.insert(path, text);
            }
        }
        Ok(Docs { texts })
    }

    pub(super) fn text(&self, path: &str) -> Option<&str> {
        self.texts.get(path).map(String::as_str)
    }

    /// Every doc that is there, by path.
    pub(super) fn each(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.texts.iter().map(|(path, text)| (*path, text.as_str()))
    }
}

/// One `### ` entry: its title, the line it opens on, and its fields.
pub(super) struct Entry {
    pub(super) line: usize,
    pub(super) title: String,
    /// Each `Field: value` line, a value running on over the lines that
    /// follow it; what stands after a blank line is the `body`, and so is a
    /// line after `Paths:` that opens with no path.
    pub(super) fields: BTreeMap<String, String>,
}

impl Entry {
    /// The globs its `Paths:` field names.
    pub(super) fn paths(&self) -> Vec<String> {
        self.fields
            .get("Paths")
            .map(|paths| spans(paths))
            .unwrap_or_default()
    }
}

/// The `### ` entries of `text`, in order.
pub(super) fn entries(text: &str) -> Vec<Entry> {
    let field = field_pattern();
    let mut entries: Vec<Entry> = Vec::new();
    let mut current: Option<String> = None;
    for (at, line) in text.lines().enumerate() {
        if let Some(title) = line.strip_prefix("### ") {
            entries.push(Entry {
                line: at + 1,
                title: title.trim().to_string(),
                fields: BTreeMap::new(),
            });
            current = None;
            continue;
        }
        if line.starts_with("## ") || line.starts_with("# ") {
            current = None;
            continue;
        }
        let Some(entry) = entries.last_mut() else {
            continue;
        };
        if let Some(found) = field.captures(line) {
            let name = found.get(1).map_or("", |name| name.as_str()).to_string();
            let value = found.get(2).map_or("", |value| value.as_str()).to_string();
            entry.fields.insert(name.clone(), value);
            current = Some(name);
        } else if line.trim().is_empty() {
            current = Some("body".to_string());
        } else {
            if current.as_deref() == Some("Paths") && !line.trim_start().starts_with('`') {
                current = Some("body".to_string());
            }
            let name = current.clone().unwrap_or_else(|| "body".to_string());
            let value = entry.fields.entry(name).or_default();
            if !value.is_empty() {
                value.push(' ');
            }
            value.push_str(line.trim());
        }
    }
    entries
}

/// A rule: a bullet, the tags that say what holds it, and the paths only a
/// review holds it on.
pub(super) struct Rule {
    pub(super) line: usize,
    pub(super) text: String,
    pub(super) tags: Vec<Tag>,
    pub(super) paths: Vec<String>,
    /// Whether the rule has a `Paths:` field, spans in it or not.
    pub(super) paths_said: bool,
}

/// What holds a rule.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Tag {
    /// A test target of a crate, and the test in it when one is named.
    Test {
        krate: String,
        target: String,
        item: Option<String>,
    },
    /// A smell counter.
    Smell(String),
    /// A review, and nothing a machine runs.
    Review,
    /// A `[test …]` tag with no crate or no target, as written.
    Malformed(String),
}

/// The bullets under the `## ` sections of `text` titled one of `sections`.
/// A rule's `Paths:` may follow its tags on the bullet's line or open a line
/// of its own, and runs on to the rule's end.
pub(super) fn rules(text: &str, sections: &[&str]) -> Vec<Rule> {
    let mut rules: Vec<Rule> = Vec::new();
    let mut inside = false;
    let mut paths_open = false;
    for (at, line) in text.lines().enumerate() {
        if let Some(title) = line.strip_prefix("## ") {
            inside = sections.contains(&title.trim());
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(bullet) = line.strip_prefix("- ") {
            rules.push(Rule {
                line: at + 1,
                text: String::new(),
                tags: Vec::new(),
                paths: Vec::new(),
                paths_said: false,
            });
            paths_open = false;
            if let Some(rule) = rules.last_mut() {
                paths_open = read_rule_line(rule, bullet, paths_open);
            }
        } else if let (Some(rule), true) = (rules.last_mut(), line.starts_with("  ")) {
            paths_open = read_rule_line(rule, line.trim(), paths_open);
        } else {
            paths_open = false;
        }
    }
    for rule in &mut rules {
        rule.tags = tags(&rule.text);
    }
    rules
}

/// Reads one line of `rule` into its text or its paths, and says whether its
/// `Paths:` is open after it.
fn read_rule_line(rule: &mut Rule, line: &str, paths_open: bool) -> bool {
    if paths_open {
        rule.paths.extend(spans(line));
        return true;
    }
    let (said, paths) = match line.split_once("Paths:") {
        Some((said, paths)) => (said.trim_end(), Some(paths)),
        None => (line, None),
    };
    if !said.is_empty() {
        if !rule.text.is_empty() {
            rule.text.push(' ');
        }
        rule.text.push_str(said);
    }
    let Some(paths) = paths else {
        return false;
    };
    rule.paths_said = true;
    rule.paths.extend(spans(paths));
    true
}

/// The tags a rule's text carries: `[test <crate>/<target>]`,
/// `[test <crate>/<target>::<test>]`, `[smell <counter>]`, `[review]`.
pub(super) fn tags(text: &str) -> Vec<Tag> {
    tag_pattern()
        .captures_iter(text)
        .map(|found| {
            let kind = found.get(1).map(|kind| kind.as_str());
            let named = found.get(2).map_or("", |named| named.as_str()).trim();
            match kind {
                Some("smell") => Tag::Smell(named.to_string()),
                Some("test") => {
                    let (target, item) = match named.split_once("::") {
                        Some((target, item)) => (target, Some(item.to_string())),
                        None => (named, None),
                    };
                    match target.split_once('/') {
                        Some((krate, target)) if !krate.is_empty() && !target.is_empty() => {
                            Tag::Test {
                                krate: krate.to_string(),
                                target: target.to_string(),
                                item,
                            }
                        }
                        _ => Tag::Malformed(format!("test {named}")),
                    }
                }
                _ => Tag::Review,
            }
        })
        .collect()
}

/// The rows of the table whose header row opens with `| <first>`: each row's
/// cells, with the line it stands on.
pub(super) fn table(text: &str, first: &str) -> Vec<(usize, Vec<String>)> {
    let header = format!("| {first} |");
    let mut rows = Vec::new();
    let mut lines = text.lines().enumerate();
    for (_, line) in lines.by_ref() {
        if line.starts_with(&header) {
            break;
        }
    }
    for (at, line) in lines.skip(1) {
        let Some(inner) = line.strip_prefix('|') else {
            break;
        };
        let cells = inner
            .trim_end_matches('|')
            .split('|')
            .map(|cell| cell.trim().to_string())
            .collect();
        rows.push((at + 1, cells));
    }
    rows
}

/// The inline code spans of `text`, in order.
pub(super) fn spans(text: &str) -> Vec<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// The inline code spans of `text`, each with the line it stands on.
pub(super) fn spans_by_line(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .flat_map(|(at, line)| spans(line).into_iter().map(move |span| (at + 1, span)))
        .collect()
}

fn field_pattern() -> &'static Regex {
    static FIELD: OnceLock<Regex> = OnceLock::new();
    FIELD.get_or_init(|| Regex::new(r"^([A-Z][A-Za-z]+): ?(.*)$").expect("a valid pattern"))
}

fn tag_pattern() -> &'static Regex {
    static TAG: OnceLock<Regex> = OnceLock::new();
    TAG.get_or_init(|| {
        Regex::new(r"\[(test|smell) ([^\]]+)\]|\[review\]").expect("a valid pattern")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_holds_its_fields_and_a_field_runs_on_over_its_lines() {
        let text = "# Traps\n\n### A test that claims concurrency\nPaths: `a/**`,\n`b.rs`\nWrong: one.\nRight: two.\n";
        let found = entries(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "A test that claims concurrency");
        assert_eq!(found[0].paths(), ["a/**", "b.rs"]);
        assert_eq!(
            found[0].fields.get("Wrong").map(String::as_str),
            Some("one.")
        );
    }

    #[test]
    fn a_line_after_the_paths_that_opens_with_no_path_is_the_body() {
        let text = "### A gap\nPaths: `a/**`,\n`b.rs`\nA run keeps `c/` aside.\n";
        let found = entries(text);
        assert_eq!(found[0].paths(), ["a/**", "b.rs"]);
        assert_eq!(
            found[0].fields.get("body").map(String::as_str),
            Some("A run keeps `c/` aside.")
        );
    }

    #[test]
    fn a_rule_carries_its_tags_and_the_paths_a_review_holds_it_on() {
        let text = "## Product rules\n\n- The engine never draws. [test engine/deps::never]\n- A pause is for a person. [review]\n  Paths: `crates/a/**`,\n  `crates/b.rs`\n\n## Other\n- not a rule\n";
        let found = rules(text, &["Product rules"]);
        assert_eq!(found.len(), 2);
        assert_eq!(
            found[0].tags,
            [Tag::Test {
                krate: "engine".into(),
                target: "deps".into(),
                item: Some("never".into())
            }]
        );
        assert_eq!(found[1].tags, [Tag::Review]);
        assert_eq!(found[1].paths, ["crates/a/**", "crates/b.rs"]);
        assert_eq!(found[1].text, "A pause is for a person. [review]");
    }

    #[test]
    fn a_rule_s_paths_may_follow_its_tags_on_its_own_line() {
        let text = "## Rules\n- A pause is for a person. [review] Paths: `crates/a/**`,\n  `crates/b.rs`\n- Bare. [review] Paths: crates/c.rs\n";
        let found = rules(text, &["Rules"]);
        assert_eq!(found[0].paths, ["crates/a/**", "crates/b.rs"]);
        assert_eq!(found[0].text, "A pause is for a person. [review]");
        assert!(found[1].paths_said);
        assert!(found[1].paths.is_empty());
    }

    #[test]
    fn a_test_tag_without_its_crate_is_kept_as_written() {
        assert_eq!(
            tags("A retry waits. [test check_retry] [review]"),
            [Tag::Malformed("test check_retry".into()), Tag::Review]
        );
    }

    #[test]
    fn a_table_is_its_rows_after_the_header() {
        let text = "| You are changing | Open |\n|---|---|\n| event kinds | `a.rs` |\n| config keys | `b.rs`, `c.rs` |\n\nafter\n";
        let rows = table(text, "You are changing");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].1, ["config keys", "`b.rs`, `c.rs`"]);
        assert_eq!(spans(&rows[1].1[1]), ["b.rs", "c.rs"]);
    }
}
