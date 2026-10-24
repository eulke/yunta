//! Where each rule of the design corpus went as it moved into the code. A
//! row of `docs/design/fates.tsv` names a source (a decision, `D53`, or a
//! heading of a living spec, `spec-events.md#5.1`), what became of it, where
//! it stands now, and the words that stand there:
//!
//! ```text
//! source<TAB>fate<TAB>destination<TAB>anchor
//! D53<TAB>code<TAB>crates/storage/src/lib.rs<TAB>no dialect leaks past this crate's interface
//! D01<TAB>history<TAB>-<TAB>the name, which nothing in the code decides
//! ```
//!
//! A fate is `code`, `test`, `agents` or `user` with a tracked destination
//! that holds the anchor, or `history` with `-` and the reason. `--check`
//! holds every row to its destination; `--check --complete` also holds the
//! corpus to its rows, every accepted decision and every heading of the
//! living specs, which is what deleting the corpus waits for.

use std::collections::BTreeSet;

use super::repo::Repo;

/// The table of fates.
const FATES: &str = "docs/design/fates.tsv";

/// The specs whose every heading is owed a fate.
const LIVING: [&str; 5] = [
    "contrato-del-run.md",
    "spec-events.md",
    "spec-adapter.md",
    "spec-tasks.md",
    "glosario.md",
];

/// What became of a rule of the corpus.
const KINDS: [&str; 5] = ["code", "test", "agents", "user", "history"];

/// One row: where a source went.
struct Fate {
    line: usize,
    source: String,
    kind: String,
    destination: String,
    anchor: String,
}

/// Every problem the fates hold, and with `complete`, every source of the
/// corpus that has no row.
pub(super) fn check(repo: &Repo, complete: bool) -> Result<Vec<String>, String> {
    let Some(text) = repo.read_if_there(FATES) else {
        return Ok(match complete {
            true => vec![format!(
                "{FATES}: is missing — every rule of the corpus is owed a row"
            )],
            false => Vec::new(),
        });
    };
    let (fates, mut problems) = rows(&text);
    let mut seen = BTreeSet::new();
    for fate in &fates {
        let at = format!("{FATES}:{}", fate.line);
        if !seen.insert(fate.source.clone()) {
            problems.push(format!(
                "{at}: `{}` has a row already — keep one",
                fate.source
            ));
        }
        if let Some(why) = unresolved_source(repo, &fate.source)? {
            problems.push(format!("{at}: {why}"));
        }
        problems.extend(destination_problem(repo, fate)?.map(|why| format!("{at}: {why}")));
    }
    if complete {
        for source in owed(repo)? {
            if !seen.contains(&source) {
                problems.push(format!(
                    "{FATES}: `{source}` has no row — say where it went, or that it is history"
                ));
            }
        }
    }
    Ok(problems)
}

/// The rows of the table, a header and `#` lines aside, with the problems of
/// any row that does not parse.
fn rows(text: &str) -> (Vec<Fate>, Vec<String>) {
    let mut fates = Vec::new();
    let mut problems = Vec::new();
    for (at, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') || line.starts_with("source\t") {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').map(str::trim).collect();
        let [source, kind, destination, anchor] = cells.as_slice() else {
            problems.push(format!(
                "{FATES}:{}: a row is four cells apart by tabs: source, fate, destination, anchor",
                at + 1
            ));
            continue;
        };
        if !KINDS.contains(kind) {
            problems.push(format!(
                "{FATES}:{}: `{kind}` is no fate — one of {}",
                at + 1,
                KINDS.join(", ")
            ));
            continue;
        }
        fates.push(Fate {
            line: at + 1,
            source: source.to_string(),
            kind: kind.to_string(),
            destination: destination.to_string(),
            anchor: anchor.to_string(),
        });
    }
    (fates, problems)
}

/// Why `source` names nothing in the corpus, or `None` when it names a
/// decision file or one heading of a living spec.
fn unresolved_source(repo: &Repo, source: &str) -> Result<Option<String>, String> {
    if let Some(number) = decision(source) {
        let prefix = format!("docs/design/adr/{number}-");
        return Ok(
            (!repo.tracked().iter().any(|path| path.starts_with(&prefix)))
                .then(|| format!("`{source}` is no decision of the corpus")),
        );
    }
    let Some((doc, heading)) = source.split_once('#') else {
        return Ok(Some(format!(
            "`{source}` is neither a decision (`D53`) nor a heading (`spec-events.md#5.1`)"
        )));
    };
    if !LIVING.contains(&doc) {
        return Ok(Some(format!(
            "`{doc}` is no living spec: {}",
            LIVING.join(", ")
        )));
    }
    let text = repo.read(&format!("docs/design/{doc}"))?;
    let matching = headings(&text)
        .into_iter()
        .filter(|title| opens(title, heading))
        .count();
    Ok(match matching {
        1 => None,
        0 => Some(format!(
            "`{source}`: no heading of `{doc}` opens with `{heading}`"
        )),
        _ => Some(format!(
            "`{source}`: more than one heading of `{doc}` opens with `{heading}` — say more of it"
        )),
    })
}

/// Why `fate`'s destination does not hold it, or `None` when it does.
fn destination_problem(repo: &Repo, fate: &Fate) -> Result<Option<String>, String> {
    if fate.kind == "history" {
        return Ok(
            (fate.destination != "-" || fate.anchor.is_empty()).then(|| {
                "a `history` row has `-` for its destination and the reason as its anchor"
                    .to_string()
            }),
        );
    }
    if fate.anchor.is_empty() {
        return Ok(Some(
            "the anchor is empty — quote the words that stand at the destination".to_string(),
        ));
    }
    if !repo.tracks(&fate.destination) {
        return Ok(Some(format!(
            "`{}` is no tracked file — name where the rule stands now",
            fate.destination
        )));
    }
    let said = prose(&repo.read(&fate.destination)?);
    Ok((!said.contains(&collapse(&fate.anchor))).then(|| {
        format!(
            "`{}` does not say `{}` — quote words that stand there, or move the rule there",
            fate.destination, fate.anchor
        )
    }))
}

/// Every source the corpus owes a row: each accepted decision, and each
/// `##` and `###` heading of the living specs, by its number when it has one.
fn owed(repo: &Repo) -> Result<Vec<String>, String> {
    let mut owed = Vec::new();
    for path in repo.tracked() {
        let Some(name) = path.strip_prefix("docs/design/adr/") else {
            continue;
        };
        let Some(number) = name
            .split('-')
            .next()
            .filter(|number| decision(number).is_some())
        else {
            continue;
        };
        if front_matter(&repo.read(path)?).any(|line| line.trim() == "status: accepted") {
            owed.push(number.to_string());
        }
    }
    for doc in LIVING {
        let text = repo.read(&format!("docs/design/{doc}"))?;
        for title in headings(&text) {
            let key = match title.split_whitespace().next() {
                Some(first) if first.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
                    first.trim_end_matches('.').to_string()
                }
                _ => title.clone(),
            };
            owed.push(format!("{doc}#{key}"));
        }
    }
    Ok(owed)
}

/// The lines of a decision's front matter, between its two `---` lines.
fn front_matter(text: &str) -> impl Iterator<Item = &str> {
    text.lines().skip(1).take_while(|line| line.trim() != "---")
}

/// `source` as a decision number, `D` and its digits.
fn decision(source: &str) -> Option<&str> {
    let digits = source.strip_prefix('D')?;
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then_some(source)
}

/// The `##` and `###` headings of a markdown text, their markers taken off.
fn headings(text: &str) -> Vec<String> {
    let mut inside_fence = false;
    let mut found = Vec::new();
    for line in text.lines() {
        if line.starts_with("```") {
            inside_fence = !inside_fence;
            continue;
        }
        if inside_fence {
            continue;
        }
        if let Some(title) = line
            .strip_prefix("### ")
            .or_else(|| line.strip_prefix("## "))
        {
            found.push(title.trim().to_string());
        }
    }
    found
}

/// Whether `title` opens with `prefix`: the whole title, or the prefix and
/// then a space, or a `.` that ends a number (`5.` but never `5.1`).
fn opens(title: &str, prefix: &str) -> bool {
    title == prefix
        || title
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with(' ') || rest.starts_with(". ") || rest == ".")
}

/// A file's text as one line of prose: comment markers and list bullets
/// taken off each line, whitespace collapsed.
fn prose(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(|line| {
            line.trim_start()
                .trim_start_matches("//!")
                .trim_start_matches("///")
                .trim_start_matches("//")
                .trim_start_matches(['#', '*', '>'])
                .trim_start_matches("- ")
        })
        .collect();
    collapse(&lines.join(" "))
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_four_cells_and_a_known_fate() {
        let (fates, problems) = rows(
            "source\tfate\tdestination\tanchor\n# a comment\nD53\tcode\ta.rs\tthe log\nD01\tlost\t-\twhy\nD02\tcode\n",
        );
        assert_eq!(fates.len(), 1);
        assert_eq!(fates[0].line, 3);
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn an_anchor_is_read_through_comment_markers_and_line_breaks() {
        let text = "//! SQLite is the only\n//! backend — no dialect\n/// leaks.\n";
        assert!(prose(text).contains(&collapse("SQLite is the only backend")));
        assert!(prose(text).contains("backend — no dialect leaks."));
    }

    #[test]
    fn a_heading_is_named_by_its_number_or_its_whole_title() {
        let text = "# Title\n## 5. Los tipos\n### 5.1 `run_created` — engine\n```\n## not a heading\n```\n## Artifacts\n";
        assert_eq!(
            headings(text),
            ["5. Los tipos", "5.1 `run_created` — engine", "Artifacts"]
        );
        assert!(opens("5.1 `run_created` — engine", "5.1"));
        assert!(!opens("5.10 `criteria_checked`", "5.1"));
        assert!(opens("5. Los tipos", "5"));
        assert!(!opens("5.1 `run_created` — engine", "5"));
        assert!(opens("Artifacts", "Artifacts"));
    }
}
