//! A check as a document carries it: everything a [`Check`] holds but the
//! lines under its row, and what names it in a fold, which are the page's.

use surco::components::{Check, Fix, Found};

/// Every check of a checklist, each as the document a program reads.
pub(crate) fn rows<S: serde::Serializer>(
    checks: &[Check],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(checks.iter().map(Row::of))
}

/// One check, as a program reads it.
#[derive(serde::Serialize)]
struct Row<'a> {
    #[serde(serialize_with = "found_word")]
    found: Found,
    subject: &'a str,
    said: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    so: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<&'a Fix>,
}

impl<'a> Row<'a> {
    fn of(check: &'a Check) -> Self {
        Row {
            found: check.found,
            subject: &check.subject,
            said: &check.said,
            so: check.so.as_deref(),
            fix: check.fix.as_ref(),
        }
    }
}

/// What a check found, named as a document names it.
fn found_word<S: serde::Serializer>(found: &Found, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(match found {
        Found::Holds => "holds",
        Found::Caution => "caution",
        Found::Problem => "problem",
        Found::NotApplicable => "not_applicable",
    })
}

#[cfg(test)]
mod tests {
    use surco::components::{Check, Found};

    use super::Row;

    #[test]
    fn a_check_names_what_it_found_as_a_word() {
        let check = Check::new(Found::NotApplicable, "chain", "nothing to check");
        let json = serde_json::to_value(Row::of(&check)).expect("a check serializes");
        assert_eq!(json["found"], "not_applicable");
    }
}
