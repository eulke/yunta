//! A decision as its own file declares it, and what a set of them owes
//! each other.
//!
//! Front-matter carries the five facts the index is made of — the
//! number, the title, the status, and the decisions this one revises
//! and is revised by. Reading files and writing the index belongs to
//! [`super`]; everything here is a function of what was read, so the
//! rules can be proved without a corpus on disk.

use std::collections::{BTreeMap, BTreeSet};

use yunta_core::yaml::Value;
use yunta_core::NonEmpty;

/// One decision, as its file declares itself.
///
/// `revised_by` is folded into [`Status`] at parse time: a decision's
/// standing and who changed it are one fact, and a type that let them
/// disagree is what let D147 stand `revised` by nobody while a commit
/// amended its body.
#[derive(Debug)]
pub struct Decision {
    pub number: u32,
    pub title: String,
    pub status: Status,
    pub revises: Vec<u32>,
    pub file: String,
}

/// How a decision stands today, and — when it no longer stands as
/// written — who said so.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Status {
    /// In force as written, revised by nobody.
    Accepted,
    /// A later decision changed part of what it decided; the note in
    /// its body says which part, and `by` names every decision that
    /// changed it.
    Revised { by: NonEmpty<u32> },
    /// A later decision withdrew it; what it decided no longer holds.
    Retired { by: NonEmpty<u32> },
}

impl Status {
    /// Every word a `status:` may say. A register whose words are open
    /// is a register nobody can fold: these three are the whole
    /// vocabulary, and a file that says anything else is refused.
    const WORDS: [&'static str; 3] = ["accepted", "revised", "retired"];

    /// How the front-matter and the index spell it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Accepted => "accepted",
            Status::Revised { .. } => "revised",
            Status::Retired { .. } => "retired",
        }
    }

    /// The status `text` names, holding `revised_by`. A word that needs
    /// a reviser and has none, or an `accepted` that names one, is a
    /// file saying two things.
    fn read(text: &str, revised_by: Vec<u32>, file: &str) -> Result<Status, String> {
        let named = || {
            revised_by
                .iter()
                .map(|number| name(*number))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match (text, NonEmpty::new(revised_by.clone())) {
            ("accepted", None) => Ok(Status::Accepted),
            ("accepted", Some(_)) => Err(format!(
                "{file}: `status` is `accepted` and `revised_by` names {} — a decision in \
                 force as written is revised by nobody",
                named()
            )),
            ("revised", Some(by)) => Ok(Status::Revised { by }),
            ("retired", Some(by)) => Ok(Status::Retired { by }),
            ("revised" | "retired", None) => Err(format!(
                "{file}: `status` is `{text}` and `revised_by` names no decision — what \
                 changed it is a decision, and it says which"
            )),
            (other, _) => Err(format!(
                "{file}: `status` is one of {}, and says `{other}`",
                Status::WORDS
                    .iter()
                    .map(|word| format!("`{word}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

impl Decision {
    /// The decisions that revised this one — empty for one in force as
    /// written. What reciprocity and the index read.
    pub fn revisers(&self) -> &[u32] {
        match &self.status {
            Status::Accepted => &[],
            Status::Revised { by } | Status::Retired { by } => by.as_slice(),
        }
    }
}

/// How a decision is named: `D` and its number, at least two digits
/// wide, the spelling every citation in the corpus uses.
pub fn name(number: u32) -> String {
    format!("D{number:02}")
}

/// The number `D164` names, for a citation or a front-matter reference.
pub fn number_of(text: &str) -> Option<u32> {
    text.strip_prefix('D')?.parse().ok()
}

/// The fields a decision declares, and nothing else.
const FIELDS: [&str; 5] = ["number", "title", "status", "revises", "revised_by"];

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

impl Decision {
    /// Reads one decision out of the bytes of `file`: `---`
    /// front-matter, then its prose.
    pub fn parse(file: &str, body: &str) -> Result<Decision, String> {
        let front = body
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .map(|(front, _)| front)
            .ok_or_else(|| format!("{file}: a decision opens with `---` front-matter"))?;
        let mapping: Value = yunta_core::yaml::parse(front)
            .map_err(|error| format!("{file}: front-matter does not parse: {error}"))?;
        let keys = mapping
            .as_mapping()
            .ok_or_else(|| format!("{file}: front-matter is a mapping of the five fields"))?;
        for key in keys.keys() {
            let key = key.as_str().unwrap_or_default();
            if !FIELDS.contains(&key) {
                return Err(format!(
                    "{file}: front-matter names `{key}`, and a decision declares {}",
                    FIELDS
                        .iter()
                        .map(|field| format!("`{field}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        let field = |name: &str| mapping.get(name);
        let declared = text(field("number"), "number", file)?;
        let number = number_of(&declared)
            .ok_or_else(|| format!("{file}: `number` is a decision, like `D164`"))?;
        let named = file
            .split_once('-')
            .and_then(|(head, _)| number_of(head))
            .ok_or_else(|| format!("{file}: a decision file is named `D<number>-<slug>.md`"))?;
        if named != number {
            return Err(format!(
                "{file}: the file is named for {} and declares {}",
                name(named),
                name(number)
            ));
        }
        Ok(Decision {
            number,
            title: text(field("title"), "title", file)?,
            status: Status::read(
                &text(field("status"), "status", file)?,
                numbers(field("revised_by"), "revised_by", file)?,
                file,
            )?,
            revises: numbers(field("revises"), "revises", file)?,
            file: file.to_string(),
        })
    }
}

/// The decisions by number, refusing a number two files claim.
pub fn by_number(found: Vec<Decision>) -> Result<BTreeMap<u32, Decision>, String> {
    let mut decisions: BTreeMap<u32, Decision> = BTreeMap::new();
    for decision in found {
        if let Some(earlier) = decisions.insert(decision.number, decision) {
            let file = &decisions[&earlier.number].file;
            return Err(format!(
                "{} is declared twice, by `{}` and `{}`",
                name(earlier.number),
                earlier.file,
                file
            ));
        }
    }
    Ok(decisions)
}

/// Proves the numbering has no hole: the decisions run from the first
/// to the last with every number in between taken.
pub fn numbering(decisions: &BTreeMap<u32, Decision>) -> Result<(), String> {
    let (Some(first), Some(last)) = (decisions.keys().next(), decisions.keys().next_back()) else {
        return Ok(());
    };
    let missing: Vec<String> = (*first..=*last)
        .filter(|number| !decisions.contains_key(number))
        .map(name)
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the decisions run from {} to {} with {} missing",
        name(*first),
        name(*last),
        missing.join(", ")
    ))
}

/// Proves every revision is recorded on both sides: what one decision
/// says it revises, the revised one says it is revised by.
pub fn reciprocals(decisions: &BTreeMap<u32, Decision>) -> Result<(), String> {
    for decision in decisions.values() {
        let here = name(decision.number);
        for revised in &decision.revises {
            let there = name(*revised);
            match decisions.get(revised) {
                None => {
                    return Err(format!(
                        "{}: {here} says it revises {there}, which no file declares",
                        decision.file
                    ))
                }
                Some(other) if !other.revisers().contains(&decision.number) => {
                    return Err(format!(
                        "{}: {here} says it revises {there}, and {there} does not say so back",
                        decision.file
                    ))
                }
                Some(_) => {}
            }
        }
        for reviser in decision.revisers() {
            let there = name(*reviser);
            match decisions.get(reviser) {
                None => {
                    return Err(format!(
                        "{}: {here} says {there} revises it, and no file declares {there}",
                        decision.file
                    ))
                }
                Some(other) if !other.revises.contains(&decision.number) => {
                    return Err(format!(
                        "{}: {here} says {there} revises it, and {there} does not say so back",
                        decision.file
                    ))
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// Proves every citation names a decision that exists, saying which
/// file cites what.
pub fn resolve(
    cited: &BTreeSet<(u32, String)>,
    decisions: &BTreeMap<u32, Decision>,
) -> Result<(), String> {
    let unresolved: Vec<String> = cited
        .iter()
        .filter(|(number, _)| !decisions.contains_key(number))
        .map(|(number, file)| format!("{} (cited by {file})", name(*number)))
        .collect();
    if unresolved.is_empty() {
        return Ok(());
    }
    Err(format!(
        "these citations name no decision: {}",
        unresolved.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(number: u32, revises: &[u32], revised_by: &[u32]) -> Decision {
        Decision {
            number,
            title: format!("La decisión {number}"),
            status: match NonEmpty::new(revised_by.to_vec()) {
                None => Status::Accepted,
                Some(by) => Status::Revised { by },
            },
            revises: revises.to_vec(),
            file: format!("{}-una-decision.md", name(number)),
        }
    }

    fn set(decisions: Vec<Decision>) -> BTreeMap<u32, Decision> {
        by_number(decisions).expect("the numbers are distinct")
    }

    fn file(number: u32, status: &str, revises: &str, revised_by: &str) -> String {
        format!(
            "---\nnumber: {}\ntitle: \"Una decisión\"\nstatus: {status}\n\
             revises: {revises}\nrevised_by: {revised_by}\n---\n\n# Una decisión\n",
            name(number)
        )
    }

    /// The status and the revisers are one fact, so a file cannot say
    /// two things: an `accepted` decision has no reviser, and a
    /// `revised` or `retired` one has at least one — the case that let
    /// D147 stand revised by nobody while a commit amended its body.
    #[test]
    fn a_status_that_disagrees_with_its_revisers_is_refused() {
        for (status, revised_by) in [("revised", "[]"), ("retired", "[]")] {
            let refused = Decision::parse("D06-un-slug.md", &file(6, status, "[]", revised_by))
                .expect_err("a revision with no reviser says nothing about who revised it");
            assert!(
                refused.contains("D06") && refused.contains(status),
                "{refused}"
            );
        }
        let refused = Decision::parse("D06-un-slug.md", &file(6, "accepted", "[]", "[D178]"))
            .expect_err("a decision in force as written has no reviser");
        assert!(
            refused.contains("D06") && refused.contains("D178"),
            "{refused}"
        );

        for (status, revised_by) in [
            ("accepted", "[]"),
            ("revised", "[D178]"),
            ("retired", "[D157]"),
        ] {
            Decision::parse("D06-un-slug.md", &file(6, status, "[]", revised_by))
                .unwrap_or_else(|refused| panic!("a file that agrees with itself: {refused}"));
        }
    }

    #[test]
    fn a_decision_is_named_by_at_least_two_digits() {
        assert_eq!(name(1), "D01");
        assert_eq!(name(45), "D45");
        assert_eq!(name(164), "D164");
        assert_eq!(number_of("D01"), Some(1));
        assert_eq!(number_of("164"), None);
    }

    #[test]
    fn front_matter_declares_the_five_fields() {
        let parsed = Decision::parse("D06-un-slug.md", &file(6, "accepted", "[D02]", "[]"))
            .expect("the five fields are there");
        assert_eq!(parsed.number, 6);
        assert_eq!(parsed.status, Status::Accepted);
        assert_eq!(parsed.revises, vec![2]);
    }

    #[test]
    fn front_matter_missing_a_field_is_refused() {
        let text = "---\nnumber: D06\ntitle: \"Una decisión\"\nstatus: accepted\nrevises: []\n---\n\n# Una decisión\n";
        let refused = Decision::parse("D06-un-slug.md", text).expect_err("`revised_by` is missing");
        assert!(refused.contains("revised_by"), "{refused}");
    }

    #[test]
    fn front_matter_naming_a_sixth_field_is_refused() {
        let text = "---\nnumber: D06\ntitle: \"Una decisión\"\nstatus: accepted\n\
                    revises: []\nrevised_by: []\nretired_by: [D156]\n---\n\n# Una decisión\n";
        let refused =
            Decision::parse("D06-un-slug.md", text).expect_err("`retired_by` is not a field");
        assert!(refused.contains("retired_by"), "{refused}");
    }

    #[test]
    fn a_status_outside_the_closed_set_is_refused() {
        let refused = Decision::parse("D06-un-slug.md", &file(6, "proposed", "[]", "[]"))
            .expect_err("`proposed` is not a status");
        assert!(
            refused.contains("`accepted`, `revised`, `retired`") && refused.contains("proposed"),
            "{refused}"
        );
        for (word, revised_by) in [
            ("accepted", "[]"),
            ("revised", "[D178]"),
            ("retired", "[D157]"),
        ] {
            Decision::parse("D06-un-slug.md", &file(6, word, "[]", revised_by))
                .expect("every status of the set is read");
        }
    }

    #[test]
    fn a_file_named_for_another_decision_is_refused() {
        let refused = Decision::parse("D07-un-slug.md", &file(6, "accepted", "[]", "[]"))
            .expect_err("the name and the number disagree");
        assert!(
            refused.contains("D07") && refused.contains("D06"),
            "{refused}"
        );
    }

    #[test]
    fn two_files_that_declare_one_number_are_refused() {
        let mut twice = decision(6, &[], &[]);
        twice.file = "D06-otro-slug.md".to_string();
        let refused =
            by_number(vec![decision(6, &[], &[]), twice]).expect_err("D06 is declared twice");
        assert!(
            refused.contains("D06") && refused.contains("D06-otro-slug.md"),
            "{refused}"
        );
    }

    #[test]
    fn a_number_missing_from_the_run_is_refused() {
        let refused = numbering(&set(vec![decision(1, &[], &[]), decision(3, &[], &[])]))
            .expect_err("D02 is missing");
        assert!(refused.contains("D02"), "{refused}");
        numbering(&set(vec![decision(1, &[], &[]), decision(2, &[], &[])]))
            .expect("a run with no hole is accepted");
    }

    #[test]
    fn a_revision_recorded_on_one_side_only_is_refused() {
        let one_sided = set(vec![decision(118, &[], &[]), decision(165, &[118], &[])]);
        let refused = reciprocals(&one_sided).expect_err("D118 does not say it was revised");
        assert!(
            refused.contains("D118") && refused.contains("D165"),
            "{refused}"
        );

        let other_way = set(vec![decision(118, &[], &[165]), decision(165, &[], &[])]);
        let refused = reciprocals(&other_way).expect_err("D165 does not say it revises");
        assert!(
            refused.contains("D118") && refused.contains("D165"),
            "{refused}"
        );

        let both = set(vec![decision(118, &[], &[165]), decision(165, &[118], &[])]);
        reciprocals(&both).expect("a revision recorded on both sides is accepted");
    }

    #[test]
    fn a_revision_of_a_decision_no_file_declares_is_refused() {
        let refused =
            reciprocals(&set(vec![decision(165, &[118], &[])])).expect_err("no file declares D118");
        assert!(refused.contains("D118"), "{refused}");
    }

    #[test]
    fn a_citation_that_names_no_decision_is_refused() {
        let decisions = set(vec![decision(1, &[], &[])]);
        let cited = BTreeSet::from([
            (1, "design/adrs.md".to_string()),
            (9, "guide.md".to_string()),
        ]);
        let refused = resolve(&cited, &decisions).expect_err("D09 is nobody's decision");
        assert!(
            refused.contains("D09") && refused.contains("guide.md"),
            "{refused}"
        );
        resolve(
            &BTreeSet::from([(1, "design/adrs.md".to_string())]),
            &decisions,
        )
        .expect("a citation that names a decision resolves");
    }
}
