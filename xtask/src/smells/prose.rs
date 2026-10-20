//! The two counters that measure what this repository says about its own
//! words and its own tense.
//!
//! Both read prose rather than code, so they walk a corpus the other
//! counters do not: every file under `crates/` and `docs/`, plus the
//! YAML and JSON of the whole tree. A term is a line's worth — a line
//! that carries one counts once, the same measure `grep -c` gives.

use std::path::{Path, PathBuf};

/// Files the vocabulary counter reads: every file under `crates/` and
/// `docs/` but an asset, the agent docs, and every `.yaml` and `.json` in the
/// tree. Build output is not source, so `target/` and `.git/` are skipped.
fn vocabulary_corpus(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in ["crates", "docs"] {
        collect(&root.join(dir), &mut out, &|path| {
            !super::corpus::is_asset(path)
        });
    }
    collect(root, &mut out, &|path| {
        path.extension()
            .is_some_and(|ext| ext == "yaml" || ext == "json")
    });
    out.extend(agent_docs(root));
    out.sort();
    out.dedup();
    out
}

/// Files the tense counter reads: Rust sources, where only `//` lines are
/// prose, the markdown of `docs/`, and the agent docs.
fn tense_corpus(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(&root.join("crates"), &mut out, &|path| {
        path.extension().is_some_and(|ext| ext == "rs")
    });
    collect(&root.join("docs"), &mut out, &|path| {
        path.extension().is_some_and(|ext| ext == "md")
    });
    out.extend(agent_docs(root));
    out.sort();
    out.dedup();
    out
}

/// The files an agent reads as this repository's own instructions and
/// knowledge: `AGENTS.md` and `CLAUDE.md` at the root, and the markdown under
/// `.agents/` but its skills, which a CLI loads as configuration.
fn agent_docs(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ["AGENTS.md", "CLAUDE.md"]
        .iter()
        .map(|name| root.join(name))
        .filter(|path| path.is_file())
        .collect();
    let skills = root.join(".agents/skills");
    collect(&root.join(".agents"), &mut out, &|path| {
        path.extension().is_some_and(|ext| ext == "md") && !path.starts_with(&skills)
    });
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>, keep: &dyn Fn(&Path) -> bool) {
    for path in super::corpus::entries(dir) {
        if path.is_dir() {
            if path.ends_with("target") || path.ends_with(".git") {
                continue;
            }
            collect(&path, out, keep);
        } else if keep(&path) {
            out.push(path);
        }
    }
}

/// The files the prose counters do not read. A document records a rule's
/// history or work that lies ahead, so it quotes the words a rule retired; a
/// negative fixture or test quotes one to prove it is refused. Each entry
/// states why it is exempt beside it.
const EXEMPT_DOCUMENTS: &[&str] = &[
    "glosario.md",
    "docs/design/adr/",
    "adrs.md",
    "docs/design/plan-de-raiz/",
    "deuda-consciente.md",
    "smoke-checklist.md",
    // A negative fixture proves a retired key is rejected, so it names the key.
    "cli/tests/fixtures/",
    // A negative test proves a retired key is rejected, so it names the key.
    "core/tests/strict_keys.rs",
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
/// backends; `plugin` is what a pack and an executor are not; `subagente`
/// is what a named agent is not; and `fence hook`, `fencehook` and
/// `yunta fence` are the old names of the rail.
pub fn banned_vocabulary(root: &Path) -> usize {
    vocabulary_corpus(root)
        .iter()
        .filter(|path| !exempt(path, EXEMPT_DOCUMENTS))
        .map(|path| {
            let text = super::read_corpus(path);
            let storage = exempt(path, &["crates/storage"]);
            text.lines()
                .filter(|line| names_by_a_retired_word(line, storage))
                .count()
        })
        .sum()
}

fn names_by_a_retired_word(line: &str, in_storage: bool) -> bool {
    let low = line.to_lowercase();
    if low.contains("role:") || ledger_names_the_tasks(&low) {
        return true;
    }
    let mut words = [
        "plugin",
        "subagente",
        "fence hook",
        "fencehook",
        "yunta fence",
    ]
    .to_vec();
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

/// Markers of a line that describes a future or a past state rather than the code.
const MARKERS: &[&str] = &["for now", "will be", "future ", "now built"];

/// Lines of prose that describe a plan or a past instead of what the
/// repository does. In Rust only a comment is prose; in markdown every
/// line is. An agent doc is rewritten in place, so a date or a "no longer"
/// in one is a log entry kept where the commit message belongs.
pub fn tense_markers(root: &Path) -> usize {
    let agent_docs = agent_docs(root);
    tense_corpus(root)
        .iter()
        .filter(|path| !exempt(path, EXEMPT_DOCUMENTS))
        .map(|path| {
            let living = agent_docs.contains(path);
            prose_lines(path, &super::read_corpus(path))
                .iter()
                .filter(|line| {
                    let low = line.to_lowercase();
                    MARKERS.iter().any(|marker| low.contains(marker))
                        || (living && logs_a_change(line))
                })
                .count()
        })
        .sum()
}

/// The prose of a file: every line of markdown, and the `//` comment of each
/// line of Rust, wherever on the line it starts.
fn prose_lines(path: &Path, text: &str) -> Vec<String> {
    if !path.extension().is_some_and(|ext| ext == "rs") {
        return text.lines().map(str::to_string).collect();
    }
    text.lines()
        .zip(super::scan::blanked_lines(text))
        .map(|(line, code)| line.chars().skip(code.chars().count()).collect::<String>())
        .filter(|comment| !comment.is_empty())
        .collect()
}

/// Whether a line of an agent doc logs a change rather than stating what
/// holds: a date, or a "no longer", outside its code spans.
fn logs_a_change(line: &str) -> bool {
    let plain: String = line.split('`').step_by(2).collect();
    let low = plain.to_lowercase();
    (low.contains("no longer") && !low.contains("no longer than")) || names_a_date(&plain)
}

/// Whether `line` holds an ISO date, `2026-10-20`.
fn names_a_date(line: &str) -> bool {
    line.as_bytes().windows(10).any(|w| {
        let digits = |range: std::ops::Range<usize>| {
            w.get(range)
                .is_some_and(|d| d.iter().all(u8::is_ascii_digit))
        };
        digits(0..4)
            && w.get(4) == Some(&b'-')
            && digits(5..7)
            && w.get(7) == Some(&b'-')
            && digits(8..10)
    })
}

/// Words Spanish prose writes and English prose does not.
const SPANISH: &[&str] = &[
    "el", "la", "los", "las", "del", "de", "en", "y", "un", "que", "una", "para", "por", "con",
    "sin", "es", "son", "cuando", "donde", "porque", "pero", "sobre", "entre", "como", "cada",
    "este", "esta", "estos", "se", "lo", "al", "su", "sus", "nunca", "también", "más", "hay", "ya",
    "sólo", "solo", "puede", "debe",
];

/// Words English prose writes and Spanish prose does not.
const ENGLISH: &[&str] = &[
    "the", "of", "and", "to", "is", "that", "it", "for", "with", "as", "by", "this", "be", "are",
    "from", "or", "an", "which", "its", "when", "what", "not", "can", "has", "have", "was", "on",
    "in", "at", "if", "but", "into", "only", "each", "must", "does", "do", "never",
];

/// Lines of prose written in Spanish where the repository writes English: the
/// markdown outside the design records `docs/design/` keeps, and the comments
/// of the Rust under `crates/` and `xtask/`. A line reads as Spanish when it
/// holds three words of [`SPANISH`], or two beside a letter English does not
/// write or with no word of [`ENGLISH`].
pub fn spanish_prose_lines(root: &Path) -> usize {
    spanish_corpus(root)
        .iter()
        .map(|path| {
            prose_lines(path, &super::read_corpus(path))
                .iter()
                .filter(|line| reads_spanish(line))
                .count()
        })
        .sum()
}

fn spanish_corpus(root: &Path) -> Vec<PathBuf> {
    let design = root.join("docs/design");
    let markdown = |path: &Path| path.extension().is_some_and(|ext| ext == "md");
    let mut out: Vec<PathBuf> = super::corpus::entries(root)
        .into_iter()
        .filter(|path| path.is_file() && markdown(path))
        .collect();
    for dir in ["docs", "packs", "crates"] {
        collect(&root.join(dir), &mut out, &|path| {
            markdown(path) && !path.starts_with(&design)
        });
    }
    for dir in ["crates", "xtask"] {
        collect(&root.join(dir), &mut out, &|path| {
            path.extension().is_some_and(|ext| ext == "rs")
        });
    }
    out.extend(agent_docs(root));
    out.sort();
    out.dedup();
    out
}

/// Whether `line` reads as Spanish, its words taken as whitespace-separated
/// tokens with the punctuation at their ends trimmed, so a path or a slug
/// stays one word.
fn reads_spanish(line: &str) -> bool {
    let low = line.to_lowercase();
    let words: Vec<&str> = low
        .split_whitespace()
        .map(|token| token.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|word| !word.is_empty())
        .collect();
    let spanish: std::collections::BTreeSet<&str> = words
        .iter()
        .copied()
        .filter(|word| SPANISH.contains(word))
        .collect();
    let english = words.iter().any(|word| ENGLISH.contains(word));
    let accented = low.contains(['ñ', 'á', 'é', 'í', 'ó', 'ú', '¿', '¡']);
    spanish.len() >= 3 || (spanish.len() >= 2 && (accented || !english))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smells::corpus::scratch_tree;

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
    fn a_retired_word_names_the_fence_hook() {
        assert!(names_by_a_retired_word(
            "the fence hook answers the write",
            false
        ));
        assert!(names_by_a_retired_word(
            "    FenceHook::command(adapter)",
            false
        ));
    }

    #[test]
    fn contains_word_takes_the_plural_and_leaves_an_identifier_alone() {
        assert!(contains_word("two backends", "backend"));
        assert!(!contains_word("a webdriver crate", "driver"));
        assert!(!contains_word("driver_name", "driver"));
    }

    #[test]
    fn a_negative_fixture_is_exempt_and_a_marker_is_read_in_present_tense() {
        let fixtures = scratch_tree(
            "fixtures",
            &[("crates/cli/tests/fixtures/typo.yaml", "role: planner\n")],
        );
        assert_eq!(banned_vocabulary(&fixtures), 0);
        let present = scratch_tree(
            "present",
            &[("crates/engine/src/a.rs", "// no run exists yet\n")],
        );
        assert_eq!(tense_markers(&present), 0);
        let future = scratch_tree(
            "future",
            &[("crates/engine/src/b.rs", "// a future resume\n")],
        );
        assert_eq!(tense_markers(&future), 1);
    }

    #[test]
    fn a_spanish_line_reads_as_spanish_and_an_english_one_does_not() {
        assert!(reads_spanish("el run no relee el workflow para cada nodo"));
        assert!(reads_spanish("una sesión que escribe"));
        assert!(reads_spanish("// Lee el log y deriva el estado"));
        assert!(reads_spanish("// Si el nodo falla, se reintenta"));
        assert!(!reads_spanish(
            "/// Whether the cell at `x`, `y` holds nothing."
        ));
        assert!(!reads_spanish(
            "see docs/design/adr/D06-una-decision-de-el.md for it"
        ));
        assert!(!reads_spanish(
            "a session writes what its scope names, per se"
        ));
        assert!(!reads_spanish("the es2015 target, solo mode, et al"));
    }

    /// The design records still stand in Spanish; anywhere else, Spanish
    /// prose counts, and only a Rust comment is prose in a source file.
    #[test]
    fn spanish_counts_outside_the_design_records_and_only_as_prose() {
        let root = scratch_tree(
            "spanish",
            &[
                (
                    "docs/design/x.md",
                    "el run no relee el workflow para cada nodo\n",
                ),
                (
                    "docs/guide.md",
                    "el run no relee el workflow para cada nodo\n",
                ),
                (
                    "crates/a/src/lib.rs",
                    "// el run no relee el workflow para cada nodo\nlet x = \"el que la\";\n",
                ),
                (".agents/open.md", "una sesión que escribe\n"),
            ],
        );
        assert_eq!(spanish_prose_lines(&root), 3);
    }

    /// An agent doc is rewritten in place, so a date or a "no longer" in one
    /// counts; the user docs keep their own history words.
    #[test]
    fn a_date_or_a_no_longer_counts_in_an_agent_doc_and_nowhere_else() {
        let root = scratch_tree(
            "living",
            &[
                ("AGENTS.md", "As of 2026-10-20 the map is here.\n"),
                (".agents/choices.md", "The engine no longer names a mode.\n"),
                (".agents/skills/x/SKILL.md", "Written 2026-10-20.\n"),
                (
                    "docs/guide.md",
                    "The flag no longer exists since 2026-10-20.\n",
                ),
            ],
        );
        assert_eq!(tense_markers(&root), 2);
        assert!(names_a_date("on 2026-10-20"));
        assert!(!names_a_date("version 0.0.5, 2026"));
    }

    #[test]
    fn an_image_under_docs_is_not_read_as_prose() {
        let root = scratch_tree("image", &[("docs/x.md", "plain words\n")]);
        std::fs::write(
            root.join("docs/x.png"),
            [0x89, 0x50, 0x4e, 0x47, 0xff, 0xfe],
        )
        .unwrap();
        assert_eq!(banned_vocabulary(&root), 0);
    }
}
