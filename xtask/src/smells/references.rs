//! Which lines of the code name a document: a decision, a section, an
//! invariant, a principle, a milestone, a debt entry, an agent doc, or a
//! document by its file name. A line that names one points a reader at a
//! record that can move or change, so it says what the code does instead.
//! The file names come from the markdown of the documentation directory; the
//! fixed forms cover what a name alone cannot. The code read is every crate
//! and pack, the repository's tooling, its scripts, its CI and its manifests.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

/// Files that read documents by name: the documents are their subject.
const EXEMPT: &[&str] = &[
    "crates/cli/tests/docs_sync.rs",
    "crates/cli/tests/tasks_spec_sync.rs",
    "crates/cli/tests/doc_claims.rs",
    "crates/core/tests/vocabulary.rs",
    // The decision index reads and writes the decisions by number.
    "xtask/src/adr.rs",
    "xtask/src/adr/",
    // The checkers spell the forms they look for.
    "xtask/src/smells/references.rs",
    "xtask/src/smells/prose.rs",
];

/// The agent docs by name. A line that names one points a reader at a page
/// rewritten in place, so the code says what it does instead.
const AGENT_DOCS: &str =
    r"\.agents/(README|architecture|choices|open|traps)\.md|\b(choices|open|traps)\.md\b";

/// Forms naming a decision, section, invariant, principle, milestone, debt id or document.
fn fixed() -> &'static Regex {
    static FIXED: OnceLock<Regex> = OnceLock::new();
    FIXED.get_or_init(|| {
        Regex::new(r"\bD[0-9]{1,3}\b|§|\bADRs?\b|\bRFC-[0-9]{4}\b|\bM[0-9]{2}\b|\bA-[0-9]{1,2}\b|\bI[0-9]{1,2}\b|\bP[0-9]{1,2}\b|docs/design|docs/(guide|packs|adapters|concepts|compatibility|troubleshooting)\.md|referencia-schema|spec-(events|adapter|tasks)|(?i:plan-de-raiz|deuda-consciente|glosario|smoke[- ]test|smoke-checklist|cerco|run contract|contrato|\bdel plan\b|the (run )?contract('s| fixes| forbids| needs| states| says)|the (run )?spec (fixes|says|states))")
            .expect("a valid pattern")
    })
}

fn agent_docs() -> &'static Regex {
    static AGENTS: OnceLock<Regex> = OnceLock::new();
    AGENTS.get_or_init(|| Regex::new(AGENT_DOCS).expect("a valid pattern"))
}

/// Whether `line` names a document: a fixed form, an agent doc, or a name in
/// `names`.
pub fn names_a_document(line: &str, names: &[String]) -> bool {
    fixed().is_match(line)
        || agent_docs().is_match(line)
        || names.iter().any(|name| line.contains(name.as_str()))
}

/// Lines of the code that name a document, outside the exempt files.
pub fn count_doc_references(root: &Path) -> usize {
    let names = document_names(&root.join("docs"));
    code_files(root)
        .into_iter()
        .filter(|path| !exempt(path))
        .map(|path| super::read_corpus(&path))
        .map(|text| {
            text.lines()
                .filter(|line| names_a_document(line, &names))
                .count()
        })
        .sum()
}

/// The code the counter reads: every file of the crates, the packs, `xtask/`,
/// `scripts/` and the repository's own `.yunta/` workflows, config and tests;
/// the YAML of `.github/`; and the root's manifests, `install.sh` and
/// `Dockerfile`. Build output and assets are not code.
fn code_files(root: &Path) -> Vec<PathBuf> {
    let dirs = [
        "crates",
        "packs",
        "xtask",
        "scripts",
        ".yunta/workflows",
        ".yunta/tests",
    ];
    let mut files: Vec<PathBuf> = dirs
        .iter()
        .flat_map(|dir| every_file(&root.join(dir)))
        .collect();
    files.extend(
        every_file(&root.join(".github"))
            .into_iter()
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext == "yml" || ext == "yaml")
            }),
    );
    let named = [
        "Cargo.toml",
        "cliff.toml",
        "deny.toml",
        "clippy.toml",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        ".yunta/config.yaml",
        "install.sh",
        "Dockerfile",
    ];
    files.extend(
        named
            .iter()
            .map(|name| root.join(name))
            .filter(|path| path.is_file()),
    );
    files
        .into_iter()
        .filter(|path| !super::corpus::is_asset(path))
        .filter(|path| {
            let inside = path.strip_prefix(root).unwrap_or(path);
            !inside.components().any(|part| part.as_os_str() == "target")
        })
        .collect()
}

fn exempt(path: &Path) -> bool {
    let shown = path.to_string_lossy().replace('\\', "/");
    EXEMPT.iter().any(|exempt| match exempt.strip_suffix('/') {
        Some(dir) => shown.contains(&format!("{dir}/")),
        None => shown.ends_with(exempt),
    })
}

/// The file names of the markdown documents under `docs`, but not its README.
fn document_names(docs: &Path) -> Vec<String> {
    every_file(docs)
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .filter_map(|path| path.file_name()?.to_str().map(str::to_string))
        .filter(|name| name != "README.md")
        .collect()
}

/// Every file under `dir`, at any depth.
fn every_file(dir: &Path) -> Vec<PathBuf> {
    super::corpus::entries(dir)
        .into_iter()
        .flat_map(|path| {
            if path.is_dir() {
                every_file(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A guard: it passes from its first commit. It pins the forms the
    /// pattern names and the plain words it must leave alone, and its ids
    /// are synthetic and out of range, so they name no record.
    #[test]
    fn names_a_document_names_only_documents_and_decisions() {
        let none: &[String] = &[];
        assert!(
            names_a_document("see docs/design/x.md", none)
                && names_a_document("D999 states", none)
                && names_a_document("§99 block", none)
                && names_a_document("P99, stated once", none)
                && names_a_document("notes: \"Ver Contrato del Run.\"", none)
                && names_a_document("// ✓ del Plan: the request is refused", none)
        );
        assert!(names_a_document("see x.md", &["x.md".to_string()]));
        assert!(
            names_a_document("// see .agents/open.md", none)
                && names_a_document("listed in traps.md", none)
                && !names_a_document("reopen.md lists it", none)
                && !names_a_document("the project's .agents/notes.md", none)
        );
        assert!(
            !names_a_document("address = 3", none)
                && !names_a_document("Dogma 3", none)
                && !names_a_document("ADRENALINE", none)
                && !names_a_document("version 2", none)
                && !names_a_document("a JSON contract over stdio", none)
        );
    }
}
