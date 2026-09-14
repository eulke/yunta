//! The decisions under `docs/design/adr/`, and the index that lists them.
//!
//! A decision is a file with front-matter, and the index in `adrs.md` is
//! derived from those files rather than kept by hand: an index written
//! twice is an index that disagrees with itself. `--check` proves the
//! committed index is what the files say, that no number is missing or
//! used twice, that every citation in the design corpus resolves, and
//! that a revision is recorded on both sides.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use yunta_core::yaml::Value;

use crate::Mode;

/// One decision, as its file declares itself.
struct Decision {
    number: u32,
    title: String,
    status: String,
    revises: Vec<u32>,
    revised_by: Vec<u32>,
    file: String,
}

/// Where the decisions and their index live.
fn design_dir() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("docs").join("design"))
        .ok_or_else(|| "the xtask crate sits directly under the workspace root".to_string())
}

/// `D164` read as `164`, for any citation or front-matter reference.
fn number_of(text: &str) -> Option<u32> {
    text.strip_prefix('D')?.parse().ok()
}

/// The list of decision numbers a front-matter field names.
fn numbers(value: Option<&Value>, field: &str, file: &str) -> Result<Vec<u32>, String> {
    let Some(value) = value else {
        return Err(format!("{file}: front-matter has no `{field}`"));
    };
    let Some(items) = value.as_sequence() else {
        return Err(format!(
            "{file}: `{field}` is a list of decisions, like `[D13]`"
        ));
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .and_then(number_of)
                .ok_or_else(|| format!("{file}: `{field}` names something that is not a decision"))
        })
        .collect()
}

/// The text of a front-matter field that must be a non-empty string.
fn text(value: Option<&Value>, field: &str, file: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .filter(|found| !found.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{file}: front-matter has no `{field}`"))
}

/// Reads one decision file: `---` front-matter, then its prose.
fn read_decision(path: &Path) -> Result<Decision, String> {
    let file = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let body = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    let front = body
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front, _)| front)
        .ok_or_else(|| format!("{file}: a decision opens with `---` front-matter"))?;
    let mapping: Value = yunta_core::yaml::parse(front)
        .map_err(|error| format!("{file}: front-matter does not parse: {error}"))?;
    let field = |name: &str| mapping.get(name);
    let declared = text(field("number"), "number", &file)?;
    let number = number_of(&declared)
        .ok_or_else(|| format!("{file}: `number` is a decision, like `D164`"))?;
    let named = file
        .split_once('-')
        .and_then(|(head, _)| number_of(head))
        .ok_or_else(|| format!("{file}: a decision file is named `D<number>-<slug>.md`"))?;
    if named != number {
        return Err(format!(
            "{file}: the file is named for D{named} and declares D{number}"
        ));
    }
    Ok(Decision {
        number,
        title: text(field("title"), "title", &file)?,
        status: text(field("status"), "status", &file)?,
        revises: numbers(field("revises"), "revises", &file)?,
        revised_by: numbers(field("revised_by"), "revised_by", &file)?,
        file,
    })
}

/// Every decision that lives in its own file, by number.
fn decisions(dir: &Path) -> Result<BTreeMap<u32, Decision>, String> {
    let mut found: BTreeMap<u32, Decision> = BTreeMap::new();
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read `{}`: {error}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot read `{}`: {error}", dir.display()))?
            .path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if !name.starts_with('D') || !name.ends_with(".md") {
            continue;
        }
        let decision = read_decision(&path)?;
        if let Some(earlier) = found.insert(decision.number, decision) {
            return Err(format!(
                "D{} is declared twice, once by `{}`",
                earlier.number, earlier.file
            ));
        }
    }
    if found.is_empty() {
        return Err(format!("`{}` holds no decision", dir.display()));
    }
    Ok(found)
}

/// The decisions the register carries in its own prose, by number: what
/// a citation may also resolve to, and where a revision of one is
/// recorded.
fn registered(register: &str) -> BTreeMap<u32, &str> {
    register
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("**D")?;
            let (digits, _) = rest.split_once(' ')?;
            digits.parse().ok().map(|number| (number, line))
        })
        .collect()
}

/// Every `D<number>` the design corpus cites, with the file that cites it.
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
            for (index, _) in body.match_indices('D') {
                let digits: String = body[index + 1..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                // `D1` in a word (`D1234abc`, `RFD12x`) is not a citation:
                // one only counts where a non-word character precedes it
                // and follows the digits.
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
                    cited.insert((number, where_from.clone()));
                }
            }
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
                        .map(|number| format!("D{number}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            format!(
                "**D{} — {}.** `{}`{revised_by} → [`adr/{}`](adr/{})\n",
                decision.number, decision.title, decision.status, decision.file, decision.file
            )
        })
        .collect()
}

/// The register with its generated section replaced by `rendered`.
fn with_index(register: &str, rendered: &str) -> Result<String, String> {
    let (head, rest) = register
        .split_once(HEADING)
        .ok_or_else(|| format!("`adrs.md` has no `{}` section", HEADING.trim()))?;
    let (prose, _) = rest
        .split_once("\n**D")
        .ok_or_else(|| "the generated section lists no decision".to_string())?;
    Ok(format!("{head}{HEADING}{prose}\n{rendered}"))
}

const HEADING: &str = "# Decisiones por archivo\n";

/// Reads the decisions, proves what they say about each other, and
/// writes or checks the index the register carries.
pub fn run(mode: Mode) -> Result<(), String> {
    let dir = design_dir()?;
    let adr_dir = dir.join("adr");
    let decisions = decisions(&adr_dir)?;

    let first = *decisions.keys().next().unwrap_or(&0);
    let last = *decisions.keys().next_back().unwrap_or(&0);
    let missing: Vec<String> = (first..=last)
        .filter(|number| !decisions.contains_key(number))
        .map(|number| format!("D{number}"))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "the decisions under `adr/` run from D{first} to D{last} with {} missing",
            missing.join(", ")
        ));
    }

    let register_path = dir.join("adrs.md");
    let register = std::fs::read_to_string(&register_path)
        .map_err(|error| format!("cannot read `{}`: {error}", register_path.display()))?;
    let registered = registered(&register);

    for decision in decisions.values() {
        for revised in &decision.revises {
            let reciprocal = match decisions.get(revised) {
                Some(other) => other.revised_by.contains(&decision.number),
                None => registered.get(revised).is_some_and(|line| {
                    line.contains(&format!("Revisada por D{}", decision.number))
                }),
            };
            if !reciprocal {
                return Err(format!(
                    "{}: D{} says it revises D{revised}, and D{revised} does not say so back",
                    decision.file, decision.number
                ));
            }
        }
        for reviser in &decision.revised_by {
            let reciprocal = decisions
                .get(reviser)
                .is_some_and(|other| other.revises.contains(&decision.number));
            if !reciprocal {
                return Err(format!(
                    "{}: D{} says D{reviser} revises it, and D{reviser} does not say so back",
                    decision.file, decision.number
                ));
            }
        }
    }

    let unresolved: Vec<String> = citations(&dir)?
        .into_iter()
        .filter(|(number, _)| !decisions.contains_key(number) && !registered.contains_key(number))
        .map(|(number, file)| format!("D{number} (cited by {file})"))
        .collect();
    if !unresolved.is_empty() {
        return Err(format!(
            "these citations name no decision: {}",
            unresolved.join(", ")
        ));
    }

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
