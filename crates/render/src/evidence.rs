//! The lines an escalation's record is shown on: one fact to a line, or
//! every fact on one line for a surface with room for exactly one.

use yunta_core::events::{Evidence, Fact};
use yunta_core::text;

/// The fact on one line: its label and value joined the way every labelled
/// value in this workspace is joined, or the value alone.
pub fn line(fact: &Fact) -> String {
    match &fact.label {
        Some(label) => text::detailed(label, &fact.value),
        None => fact.value.trim().to_string(),
    }
}

/// One rendered line per fact, for a surface with room to list them.
/// Empty when nothing is attached.
pub fn lines(evidence: &Evidence) -> Vec<String> {
    evidence
        .facts()
        .iter()
        .map(line)
        .filter(|line| !line.is_empty())
        .collect()
}

/// Every fact on one line, for a surface with room for exactly one. Empty
/// when nothing is attached.
pub fn one_line(evidence: &Evidence) -> String {
    lines(evidence).join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_labelled_fact_puts_its_label_in_front_of_its_value() {
        assert_eq!(
            line(&Fact::labelled("limits.max_tokens_per_run", "400")),
            "limits.max_tokens_per_run: 400"
        );
        assert_eq!(line(&Fact::bare("exit 1")), "exit 1");
    }

    #[test]
    fn facts_read_as_one_line_in_the_order_they_were_attached() {
        let evidence = Evidence::from(vec![
            Fact::labelled("limits.max_tokens_per_run", "400"),
            Fact::labelled("spent", "500"),
        ]);
        assert_eq!(
            one_line(&evidence),
            "limits.max_tokens_per_run: 400; spent: 500"
        );
        assert_eq!(
            lines(&evidence),
            vec!["limits.max_tokens_per_run: 400", "spent: 500"]
        );
    }

    #[test]
    fn a_record_stored_as_one_string_still_reads_as_its_one_fact() {
        let stored = Evidence::Prose("exit 1".into());
        assert_eq!(one_line(&stored), "exit 1");
        assert_eq!(lines(&stored), vec!["exit 1"]);
        assert_eq!(one_line(&Evidence::none()), "");
    }
}
