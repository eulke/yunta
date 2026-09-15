//! The decisions under `docs/design/adr/`, and the index that lists them.
//!
//! A decision is a file with front-matter, and `adrs.md` is derived from
//! those files rather than kept by hand: an index written twice is an
//! index that disagrees with itself. `--check` proves the committed
//! index is what the files say, that no number is missing or used
//! twice, that every citation in `docs/` resolves, and that a revision
//! is recorded on both sides.

mod decision;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::Mode;
use decision::{by_number, name, numbering, reciprocals, resolve, Decision};

/// The workspace root — the parent of this crate's directory.
fn workspace_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "the xtask crate sits directly under the workspace root".to_string())
}

/// Every document a citation may live in.
fn docs_dir() -> Result<PathBuf, String> {
    workspace_root().map(|root| root.join("docs"))
}

/// Where the decisions and their index live.
fn design_dir() -> Result<PathBuf, String> {
    docs_dir().map(|docs| docs.join("design"))
}

/// Every decision that lives in its own file, by number.
fn decisions(dir: &Path) -> Result<BTreeMap<u32, Decision>, String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read `{}`: {error}", dir.display()))?;
    let mut found = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot read `{}`: {error}", dir.display()))?
            .path();
        let file = path.file_name().unwrap_or_default().to_string_lossy();
        if !file.starts_with('D') || !file.ends_with(".md") {
            continue;
        }
        let body = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
        found.push(Decision::parse(&file, &body)?);
    }
    if found.is_empty() {
        return Err(format!("`{}` holds no decision", dir.display()));
    }
    by_number(found)
}

/// Every `D<number>` a text cites.
///
/// `D1` inside a word (`D1234abc`, `RFD12x`) is not a citation: one
/// only counts where a non-word character precedes it and follows the
/// digits.
fn cited_in(body: &str) -> BTreeSet<u32> {
    let mut cited = BTreeSet::new();
    for (index, _) in body.match_indices('D') {
        let digits: String = body[index + 1..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let before_is_word = index
            .checked_sub(1)
            .and_then(|at| body[..=at].chars().next_back())
            .is_some_and(|char| char.is_alphanumeric() || char == '_');
        let after = body[index + 1 + digits.len()..].chars().next();
        if digits.is_empty()
            || before_is_word
            || after.is_some_and(|char| char.is_alphanumeric() || char == '_')
        {
            continue;
        }
        if let Ok(number) = digits.parse() {
            cited.insert(number);
        }
    }
    cited
}

/// Every `D<number>` the documentation cites, with the file that cites it.
fn citations(dir: &Path) -> Result<BTreeSet<(u32, String)>, String> {
    let mut cited = BTreeSet::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(current) = dirs.pop() {
        let entries = std::fs::read_dir(&current)
            .map_err(|error| format!("cannot read `{}`: {error}", current.display()))?;
        for entry in entries {
            let path = entry
                .map_err(|error| format!("cannot read `{}`: {error}", current.display()))?
                .path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "md") {
                continue;
            }
            let body = std::fs::read_to_string(&path)
                .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
            let where_from = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .display()
                .to_string();
            cited.extend(
                cited_in(&body)
                    .into_iter()
                    .map(|number| (number, where_from.clone())),
            );
        }
    }
    Ok(cited)
}

/// The index as the decision files say it: number, title, status, who
/// revised it, and where to read it.
fn index(decisions: &BTreeMap<u32, Decision>) -> String {
    decisions
        .values()
        .map(|decision| {
            let revised_by = match decision.revised_by.as_slice() {
                [] => String::new(),
                numbers => format!(
                    " *(Revisada por {}.)*",
                    numbers
                        .iter()
                        .map(|number| name(*number))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            format!(
                "**{} — {}.** `{}`{revised_by} → [`adr/{}`](adr/{})\n",
                name(decision.number),
                decision.title,
                decision.status.as_str(),
                decision.file,
                decision.file
            )
        })
        .collect()
}

/// The register with its list of decisions replaced by `rendered`: its
/// heading and the preamble under it are the register's own, and every
/// line from the first decision on is generated.
fn with_index(register: &str, rendered: &str) -> Result<String, String> {
    let (head, rest) = register.split_once(HEADING).ok_or_else(|| {
        format!(
            "`adrs.md` opens with `{}`, and the index follows its preamble",
            HEADING.trim()
        )
    })?;
    let (prose, _) = rest
        .split_once("\n**D")
        .ok_or_else(|| "`adrs.md` carries no decision line for the index to replace".to_string())?;
    Ok(format!("{head}{HEADING}{prose}\n{rendered}"))
}

const HEADING: &str = "# Decisiones y racionales (ADRs)\n";

/// Reads the decisions, proves what they say about each other and about
/// what the documentation cites, and writes or checks the index.
pub fn run(mode: Mode) -> Result<(), String> {
    let design = design_dir()?;
    let decisions = decisions(&design.join("adr"))?;
    numbering(&decisions)?;
    reciprocals(&decisions)?;
    resolve(&citations(&docs_dir()?)?, &decisions)?;

    let register_path = design.join("adrs.md");
    let register = std::fs::read_to_string(&register_path)
        .map_err(|error| format!("cannot read `{}`: {error}", register_path.display()))?;
    let rendered = with_index(&register, &index(&decisions))?;
    match mode {
        Mode::Write => {
            std::fs::write(&register_path, &rendered)
                .map_err(|error| format!("cannot write `{}`: {error}", register_path.display()))?;
            println!("wrote {}", register_path.display());
            Ok(())
        }
        Mode::Check if rendered == register => {
            println!("adr: {} decisions, index in step", decisions.len());
            Ok(())
        }
        Mode::Check => Err(format!(
            "`{}` differs from what the decision files say — run `cargo xtask adr`",
            register_path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::decision::Status;
    use super::*;

    fn decision(number: u32, status: Status, revised_by: &[u32]) -> Decision {
        Decision {
            number,
            title: format!("La decisión {number}"),
            status,
            revises: Vec::new(),
            revised_by: revised_by.to_vec(),
            file: format!("{}-una-decision.md", name(number)),
        }
    }

    fn set(decisions: Vec<Decision>) -> BTreeMap<u32, Decision> {
        by_number(decisions).expect("the numbers are distinct")
    }

    #[test]
    fn a_number_inside_a_word_is_not_a_citation() {
        assert_eq!(cited_in("D164 y D45."), BTreeSet::from([164, 45]));
        assert_eq!(cited_in("DO-D42 lo cita"), BTreeSet::from([42]));
        assert_eq!(cited_in("RFD12x, D1234abc, Dx"), BTreeSet::new());
        assert_eq!(cited_in("(D01)"), BTreeSet::from([1]));
    }

    #[test]
    fn the_index_names_a_decision_its_status_its_revisers_and_its_file() {
        let rendered = index(&set(vec![
            decision(6, Status::Accepted, &[]),
            decision(45, Status::Revised, &[162, 164]),
        ]));
        assert_eq!(
            rendered,
            "**D06 — La decisión 6.** `accepted` → \
             [`adr/D06-una-decision.md`](adr/D06-una-decision.md)\n\
             **D45 — La decisión 45.** `revised` *(Revisada por D162, D164.)* → \
             [`adr/D45-una-decision.md`](adr/D45-una-decision.md)\n"
        );
    }

    #[test]
    fn the_generated_section_replaces_the_one_the_register_carries() {
        let register = format!("{HEADING}\nUn párrafo.\n\n**D01 — Vieja.** `accepted`\n");
        let rendered = with_index(&register, "**D01 — Nueva.** `accepted`\n")
            .expect("the register has its heading");
        assert_eq!(
            rendered,
            format!("{HEADING}\nUn párrafo.\n\n**D01 — Nueva.** `accepted`\n")
        );
    }

    #[test]
    fn a_register_without_the_generated_section_is_refused() {
        let refused = with_index("# Otra cosa\n", "**D01 — Nueva.**\n")
            .expect_err("the heading is not there");
        assert!(refused.contains("Decisiones y racionales"), "{refused}");
    }
}
