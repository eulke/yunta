//! Which lines of the code name a document: a decision, a section, an
//! invariant, a principle, a milestone, a debt entry, or a document by its
//! file name. A comment that names one points a reader at a record that can
//! move or change, so the code says what it does instead. The file names come
//! from the markdown of the documentation directory; the fixed forms cover what
//! a name alone cannot.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

/// Test files that read documents by name: the documents are their subject.
const EXEMPT: &[&str] = &[
    "crates/cli/tests/docs_sync.rs",
    "crates/cli/tests/tasks_spec_sync.rs",
    "crates/cli/tests/doc_claims.rs",
    "crates/core/tests/vocabulary.rs",
];

/// Forms naming a decision, section, invariant, principle, milestone, debt id or document.
fn fixed() -> &'static Regex {
    static FIXED: OnceLock<Regex> = OnceLock::new();
    FIXED.get_or_init(|| {
        Regex::new(r"\bD[0-9]{1,3}\b|§|\bADRs?\b|\bRFC-[0-9]{4}\b|\bM[0-9]{2}\b|\bA-[0-9]{1,2}\b|\bI[0-9]{1,2}\b|\bP[0-9]{1,2}\b|docs/design|docs/(guide|packs|adapters|concepts|compatibility|troubleshooting)\.md|referencia-schema|spec-(events|adapter|tasks)|(?i:plan-de-raiz|deuda-consciente|glosario|smoke[- ]test|smoke-checklist|cerco|run contract|contrato|\bdel plan\b|the (run )?contract('s| fixes| forbids| needs| states| says)|the (run )?spec (fixes|says|states))")
            .expect("a valid pattern")
    })
}

/// Whether `line` names a document: a fixed form, or a name in `names`.
pub fn names_a_document(line: &str, names: &[String]) -> bool {
    fixed().is_match(line) || names.iter().any(|name| line.contains(name.as_str()))
}

/// Lines of `crates/` and `packs/` that name a document, outside the exempt files.
pub fn count_doc_references(root: &Path) -> usize {
    let names = document_names(&root.join("docs"));
    every_file(&root.join("crates"))
        .into_iter()
        .chain(every_file(&root.join("packs")))
        .filter(|path| !EXEMPT.iter().any(|exempt| path.ends_with(exempt)))
        .map(|path| super::read_corpus(&path))
        .map(|text| {
            text.lines()
                .filter(|line| names_a_document(line, &names))
                .count()
        })
        .sum()
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
            !names_a_document("address = 3", none)
                && !names_a_document("Dogma 3", none)
                && !names_a_document("ADRENALINE", none)
                && !names_a_document("version 2", none)
                && !names_a_document("a JSON contract over stdio", none)
        );
    }
}
