//! The digest `--about` prints for a set of files: for each, the module chain
//! from its crate's root down to it, and the rules, traps, open work and tests
//! whose paths select it. What an agent CLI loads as its configuration, a
//! skill a run mounts among it, is no doc's subject, so it is left out.

use yunta_core::protected::AGENT_CONFIG;
use yunta_core::ScopeGlob;

use super::agents::{self, Docs, Entry, Rule, RULE_SETS};
use super::checks::{reach, rust_files, Reach};
use super::repo::Repo;

/// The digest of `paths`, one block per path.
pub(super) fn digest(repo: &Repo, docs: &Docs, paths: &[String]) -> Result<String, String> {
    let known = Known::read(repo, docs)?;
    Ok(shown(paths)
        .map(|path| block(repo, path, &known.about(path)))
        .collect())
}

/// The paths of `paths` a digest speaks of: all but what an agent CLI loads
/// as its configuration.
fn shown(paths: &[String]) -> impl Iterator<Item = &str> {
    let config: Vec<_> = AGENT_CONFIG
        .iter()
        .filter_map(|glob| glob.parse::<ScopeGlob>().ok())
        .map(|glob| glob.compiled().compile_matcher())
        .collect();
    paths
        .iter()
        .map(String::as_str)
        .filter(move |path| !config.iter().any(|glob| glob.is_match(path)))
}

/// One path's block: the path, its module chain, and what the docs record
/// about it.
fn block(repo: &Repo, path: &str, records: &[String]) -> String {
    let mut out = format!("{path}\n");
    if let Some(chain) = module_chain(&|file: &str| repo.holds(file), path) {
        out += &format!("  modules  {}\n", chain.join(" › "));
    }
    if records.is_empty() {
        out += "  rules    none recorded for this path\n";
    }
    for record in records {
        out += &format!("  {record}\n");
    }
    out
}

/// What the docs and the tests record about paths, read once.
struct Known {
    rules: Vec<(&'static str, Rule)>,
    traps: Vec<Entry>,
    open: Vec<Entry>,
    reach: Vec<(String, Reach)>,
}

impl Known {
    fn read(repo: &Repo, docs: &Docs) -> Result<Known, String> {
        let mut rules: Vec<(&'static str, Rule)> = Vec::new();
        for (file, sections) in RULE_SETS {
            if let Some(text) = docs.text(file) {
                rules.extend(
                    agents::rules(text, sections)
                        .into_iter()
                        .map(|rule| (file, rule)),
                );
            }
        }
        let entries_of = |file: &str| docs.text(file).map(agents::entries).unwrap_or_default();
        let mut reached = Vec::new();
        for file in rust_files(repo) {
            let text = repo.read(file)?;
            reached.extend(
                reach(&text)
                    .into_iter()
                    .map(|note| (file.to_string(), note)),
            );
        }
        Ok(Known {
            rules,
            traps: entries_of(".agents/traps.md"),
            open: entries_of(".agents/open.md"),
            reach: reached,
        })
    }

    /// What the docs and the tests record about `path`, a line each.
    fn about(&self, path: &str) -> Vec<String> {
        let mut records = Vec::new();
        for (file, rule) in &self.rules {
            if selects(&rule.paths, path) {
                records.push(format!(
                    "rule     {}  {file}:{}",
                    first_sentence(&rule.text),
                    rule.line
                ));
            }
        }
        for (label, file, entries) in [
            ("trap", ".agents/traps.md", &self.traps),
            ("open", ".agents/open.md", &self.open),
        ] {
            for entry in entries.iter().filter(|entry| selects(&entry.paths(), path)) {
                records.push(format!("{label:<8} {}  {file}:{}", entry.title, entry.line));
            }
        }
        for (file, note) in &self.reach {
            if selects(&note.paths, path) {
                records.push(format!(
                    "reached  {} holds a rule this path can break  {file}:{}",
                    note.test, note.line
                ));
            }
        }
        records
    }
}

/// Whether one of `globs` selects `path`.
fn selects(globs: &[String], path: &str) -> bool {
    globs.iter().any(|glob| {
        glob.parse::<ScopeGlob>()
            .is_ok_and(|glob| glob.compiled().compile_matcher().is_match(path))
    })
}

/// The module files from a Rust file's crate root down to it: the root from
/// the repository's root, then each module file from its parent's directory.
/// `holds` says whether a path is a tracked file.
fn module_chain(holds: &dyn Fn(&str) -> bool, path: &str) -> Option<Vec<String>> {
    let (krate, below) = path.split_once("/src/")?;
    if !path.ends_with(".rs") {
        return None;
    }
    let src = format!("{krate}/src");
    let root = ["lib.rs", "main.rs"]
        .into_iter()
        .find(|root| holds(&format!("{src}/{root}")))?;
    let mut chain = vec![format!("{src}/{root}")];
    let mut dir = src;
    let parts: Vec<&str> = below.split('/').collect();
    let (file, dirs) = parts.split_last()?;
    for part in dirs {
        if holds(&format!("{dir}/{part}.rs")) {
            chain.push(format!("{part}.rs"));
        } else if holds(&format!("{dir}/{part}/mod.rs")) {
            chain.push(format!("{part}/mod.rs"));
        }
        dir = format!("{dir}/{part}");
    }
    let is_the_root = dirs.is_empty() && *file == root;
    let is_its_dir_module = *file == "mod.rs"
        && dirs
            .last()
            .is_some_and(|last| chain.last() == Some(&format!("{last}/mod.rs")));
    if !is_the_root && !is_its_dir_module {
        chain.push(file.to_string());
    }
    Some(chain)
}

/// A rule's first sentence, its tags taken off.
fn first_sentence(text: &str) -> String {
    let plain = text.split(" [").next().unwrap_or(text);
    match plain.find(". ") {
        Some(end) => plain[..=end].trim().to_string(),
        None => plain.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_reads_as_its_first_sentence_without_its_tags() {
        assert_eq!(
            first_sentence("The engine never draws. The CLI does. [test engine/deps]"),
            "The engine never draws."
        );
        assert_eq!(
            first_sentence("A pause is for a person [review]"),
            "A pause is for a person"
        );
    }

    #[test]
    fn a_module_chain_runs_from_the_crate_root_down_to_the_file() {
        let tree = [
            "crates/e/src/lib.rs",
            "crates/e/src/run/mod.rs",
            "crates/e/src/run/loop_exec.rs",
            "crates/e/src/run/loop_exec/batch.rs",
            "crates/e/src/scope_expansion.rs",
            "crates/e/src/scope_expansion/expansion.rs",
        ];
        let holds = |path: &str| tree.contains(&path);
        let chain = |path: &str| module_chain(&holds, path).unwrap_or_default();
        assert_eq!(
            chain("crates/e/src/run/loop_exec/batch.rs"),
            [
                "crates/e/src/lib.rs",
                "run/mod.rs",
                "loop_exec.rs",
                "batch.rs"
            ]
        );
        assert_eq!(
            chain("crates/e/src/run/mod.rs"),
            ["crates/e/src/lib.rs", "run/mod.rs"]
        );
        assert_eq!(
            chain("crates/e/src/scope_expansion/expansion.rs"),
            ["crates/e/src/lib.rs", "scope_expansion.rs", "expansion.rs"]
        );
        assert_eq!(chain("crates/e/src/lib.rs"), ["crates/e/src/lib.rs"]);
    }

    #[test]
    fn what_a_cli_loads_as_its_configuration_is_left_out() {
        let paths = [
            ".claude/skills/notes/SKILL.md".to_string(),
            ".agents/skills/notes/SKILL.md".to_string(),
            ".agents/open.md".to_string(),
            "crates/e/src/lib.rs".to_string(),
        ];
        assert_eq!(
            shown(&paths).collect::<Vec<_>>(),
            [".agents/open.md", "crates/e/src/lib.rs"]
        );
    }

    #[test]
    fn a_glob_selects_the_paths_it_names() {
        let globs = ["crates/*/tests/**".to_string()];
        assert!(selects(&globs, "crates/engine/tests/run.rs"));
        assert!(!selects(&globs, "crates/engine/src/run.rs"));
    }
}
