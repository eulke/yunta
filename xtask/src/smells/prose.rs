//! The two counters that measure what this repository says about its own
//! words and its own tense.
//!
//! Both read prose rather than code, so they walk a corpus the other
//! counters do not: every file under `crates/` and `docs/`, plus the
//! YAML and JSON of the whole tree. A term is a line's worth — a line
//! that carries one counts once, the same measure `grep -c` gives.

use std::path::{Path, PathBuf};

/// Files the vocabulary counter reads: everything under `crates/` and
/// `docs/`, plus every `.yaml` and `.json` in the tree. Build output is
/// not source, so `target/` and `.git/` are skipped.
fn vocabulary_corpus(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in ["crates", "docs"] {
        collect(&root.join(dir), &mut out, &|_| true);
    }
    collect(root, &mut out, &|path| {
        path.extension()
            .is_some_and(|ext| ext == "yaml" || ext == "json")
    });
    out.sort();
    out.dedup();
    out
}

/// Files the tense counter reads: Rust sources, where only `//` lines are
/// prose, and the markdown of `docs/`.
fn tense_corpus(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(&root.join("crates"), &mut out, &|path| {
        path.extension().is_some_and(|ext| ext == "rs")
    });
    collect(&root.join("docs"), &mut out, &|path| {
        path.extension().is_some_and(|ext| ext == "md")
    });
    out.sort();
    out.dedup();
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>, keep: &dyn Fn(&Path) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name == "target" || name == ".git" {
                continue;
            }
            collect(&path, out, keep);
        } else if keep(&path) {
            out.push(path);
        }
    }
}

/// The documents that state the rule, or record the words it retired:
/// the table itself, the glossary that defines each term, the decisions,
/// which quote the spelling they retired, and their index, where a title
/// carries that spelling too.
const VOCABULARY_EXEMPT: &[&str] = &["CLAUDE.md", "glosario.md", "docs/design/adr/", "adrs.md"];

/// The documents whose subject is what the repository will do: the plan
/// and its mechanisms, the debt it carries knowingly, the checklist of a
/// release ahead, and the decisions, which cite the alternative they
/// discarded. Everywhere else the text is in the present.
const TENSE_EXEMPT: &[&str] = &[
    "docs/design/plan-de-raiz/",
    "deuda-consciente.md",
    "smoke-checklist.md",
    "docs/design/adr/",
];

fn exempt(path: &Path, list: &[&str]) -> bool {
    let shown = path.to_string_lossy().replace('\\', "/");
    list.iter().any(|marker| shown.contains(marker))
}

/// Lines naming a thing by a word this repository retired for it.
///
/// `ledger` names a fold of the log and nothing else, so it counts only
/// where it stands in front of the tasks it used to name — `TaskLedger`,
/// the fold, is what the word is for. `role:` is the retired spelling of
/// `runner:`; `driver` and `backend` are what an adapter is not, except
/// in `crates/storage`, which really does choose between database
/// backends; `plugin` is what a pack and an executor are not; and
/// `subagente` is what a named agent is not.
pub fn banned_vocabulary(root: &Path) -> usize {
    vocabulary_corpus(root)
        .iter()
        .filter(|path| !exempt(path, VOCABULARY_EXEMPT))
        .filter_map(|path| {
            let text = std::fs::read_to_string(path).ok()?;
            let storage = exempt(path, &["crates/storage"]);
            Some(
                text.lines()
                    .filter(|line| names_by_a_retired_word(line, storage))
                    .count(),
            )
        })
        .sum()
}

fn names_by_a_retired_word(line: &str, in_storage: bool) -> bool {
    let low = line.to_lowercase();
    if low.contains("role:") || ledger_names_the_tasks(&low) {
        return true;
    }
    let mut words = ["plugin", "subagente"].to_vec();
    if !in_storage {
        words.extend(["driver", "backend"]);
    }
    words.iter().any(|word| contains_word(&low, word))
}

/// Whether `ledger` stands in front of the tasks document — the reading
/// this repository retired. What follows may be a possessive, a hyphen
/// or a space, so `ledger task`, `ledger-tasks` and `ledger's tasks`
/// all count, and `TaskLedger`, where the word names the fold, does not.
fn ledger_names_the_tasks(low: &str) -> bool {
    let mut rest = low;
    while let Some(at) = rest.find("ledger") {
        let after = &rest[at + "ledger".len()..];
        let after = after.trim_start_matches(['\'', 's', '-', '_', ' ']);
        if after.starts_with("task") || after.starts_with("del documento") {
            return true;
        }
        rest = &rest[at + "ledger".len()..];
    }
    false
}

/// Whether `low` carries `word` with no letter, digit or underscore on
/// either side, so `backend` counts and `backends` does too, while
/// `driver` inside an identifier a dependency owns does not.
fn contains_word(low: &str, word: &str) -> bool {
    let bytes = low.as_bytes();
    let mut from = 0;
    while let Some(at) = low[from..].find(word).map(|i| i + from) {
        let before = at.checked_sub(1).map(|i| bytes[i]);
        let after = bytes.get(at + word.len()).copied();
        let boundary = |c: Option<u8>| !c.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_');
        // A plural is the same word.
        let after = if after == Some(b's') {
            bytes.get(at + word.len() + 1).copied()
        } else {
            after
        };
        if boundary(before) && boundary(after) {
            return true;
        }
        from = at + word.len();
    }
    false
}

/// The words that put a text somewhere other than the present: what the
/// repository does not do for now, what it does not do yet, what it used
/// to do, and what it is going to do.
const MARKERS: &[&str] = &[
    "for now",
    "not yet",
    " yet",
    "today",
    "now built",
    "the old",
    "will be",
    "future ",
];

/// Lines of prose that describe a plan or a past instead of what the
/// repository does. In Rust only a comment is prose; in markdown every
/// line is.
pub fn tense_markers(root: &Path) -> usize {
    tense_corpus(root)
        .iter()
        .filter(|path| !exempt(path, TENSE_EXEMPT))
        .filter_map(|path| {
            let text = std::fs::read_to_string(path).ok()?;
            let prose_only = path.extension().is_some_and(|ext| ext == "rs");
            Some(
                text.lines()
                    .filter(|line| !prose_only || line.trim_start().starts_with("//"))
                    .filter(|line| {
                        let low = line.to_lowercase();
                        MARKERS.iter().any(|marker| low.contains(marker))
                    })
                    .count(),
            )
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_counts_in_front_of_the_tasks_and_never_behind_them() {
        assert!(ledger_names_the_tasks("the ledger's tasks, each as done"));
        assert!(ledger_names_the_tasks("one more task on the ledger task"));
        assert!(ledger_names_the_tasks("el ledger del documento de tareas"));
        // The fold of task events is what the word is for.
        assert!(!ledger_names_the_tasks(
            "`taskledger`, `gateledger`, `nodeledger`"
        ));
        assert!(!ledger_names_the_tasks(
            "a spelling of `task-ledger` a log carries"
        ));
    }

    #[test]
    fn a_retired_word_counts_and_a_word_that_contains_it_does_not() {
        assert!(names_by_a_retired_word("    role: planner", false));
        assert!(names_by_a_retired_word("//! a pack is not a plugin", false));
        assert!(names_by_a_retired_word("the only backend is sqlite", false));
        // `crates/storage` is where a database backend is the subject.
        assert!(!names_by_a_retired_word("the only backend is sqlite", true));
        // `runner:` is the spelling, and `roles` in prose is not `role:`.
        assert!(!names_by_a_retired_word("    runner: planner", false));
        assert!(!names_by_a_retired_word("dos roles y un adapter", false));
    }

    #[test]
    fn contains_word_takes_the_plural_and_leaves_an_identifier_alone() {
        assert!(contains_word("two backends", "backend"));
        assert!(!contains_word("a webdriver crate", "driver"));
        assert!(!contains_word("driver_name", "driver"));
    }
}
