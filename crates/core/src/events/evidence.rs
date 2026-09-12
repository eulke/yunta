//! What the engine attached to an escalation so a person can audit its
//! summary against the run's own log.

use serde::{Deserialize, Serialize};

use crate::text;

/// One fact the engine read off the log: the label a reader scans for,
/// and the value under it.
///
/// The label is optional because not every fact needs one. A cause like
/// `exit 1` or a sentence like `no forge reachable from this machine`
/// already names itself, and putting a word in front of it would add
/// reading without adding meaning; a bare number like `400` says
/// nothing until `limits.max_tokens_per_run` is in front of it. A fact
/// that carries a label states it here rather than writing it into the
/// value, so every surface sets the two apart the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Fact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub value: String,
}

impl Fact {
    /// A labelled fact — the shape for a value that needs a word in
    /// front of it to mean anything.
    pub fn labelled(label: impl Into<String>, value: impl Into<String>) -> Self {
        Fact {
            label: Some(label.into()),
            value: value.into(),
        }
    }

    /// A fact that names itself, kept as the engine read it.
    pub fn bare(value: impl Into<String>) -> Self {
        Fact {
            label: None,
            value: value.into(),
        }
    }

    /// The fact on one line: its label and value joined the way every
    /// labelled value in this workspace is joined, or the value alone.
    pub fn line(&self) -> String {
        match &self.label {
            Some(label) => text::detailed(label, &self.value),
            None => self.value.trim().to_string(),
        }
    }
}

/// The facts behind an escalation's summary, attached by the engine
/// straight from the log.
///
/// The pair is the point: the summary is a *claim* about what happened
/// and this is the *record* it is audited against, so neither one
/// repeats the other. A producer that writes the cause into both leaves
/// every surface printing it twice, under a heading that promised
/// something new.
///
/// Untagged, with `Prose` last: a payload carrying a list reads as
/// [`Evidence::Facts`], and a log written before evidence was data
/// carries one string and reads back as [`Evidence::Prose`], which
/// renders as the single unlabelled fact it always was. That tolerance
/// is the rule for what is persisted and versioned, and it is why no
/// reader needs to know which version wrote the line it is looking at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Evidence {
    /// What this binary writes: the labelled facts, in the order a
    /// reader meets them.
    Facts(Vec<Fact>),
    /// One string, as an older log recorded it. Only reading produces
    /// this; nothing in this workspace writes one.
    Prose(String),
}

impl Evidence {
    /// Nothing attached — the shape for an escalation whose summary is
    /// the whole of what the log says.
    pub fn none() -> Self {
        Evidence::Facts(Vec::new())
    }

    /// The facts, with an older log's prose read as the one unlabelled
    /// fact it is.
    pub fn facts(&self) -> Vec<Fact> {
        match self {
            Evidence::Facts(facts) => facts.clone(),
            Evidence::Prose(text) => vec![Fact::bare(text.clone())],
        }
    }

    /// One rendered line per fact, for a surface with room to list
    /// them.
    pub fn lines(&self) -> Vec<String> {
        self.facts()
            .iter()
            .map(Fact::line)
            .filter(|line| !line.is_empty())
            .collect()
    }

    /// Every fact on one line, for a surface with room for exactly one.
    /// Empty when nothing is attached.
    pub fn one_line(&self) -> String {
        self.lines().join("; ")
    }

    /// Whether there is anything to show.
    pub fn is_empty(&self) -> bool {
        self.lines().is_empty()
    }
}

impl From<Vec<Fact>> for Evidence {
    fn from(facts: Vec<Fact>) -> Self {
        Evidence::Facts(facts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_labelled_fact_puts_its_label_in_front_of_its_value() {
        assert_eq!(
            Fact::labelled("limits.max_tokens_per_run", "400").line(),
            "limits.max_tokens_per_run: 400"
        );
    }

    #[test]
    fn a_fact_that_names_itself_is_read_as_it_was_written() {
        assert_eq!(Fact::bare("exit 1").line(), "exit 1");
    }

    #[test]
    fn facts_read_as_one_line_in_the_order_they_were_attached() {
        let evidence = Evidence::from(vec![
            Fact::labelled("limits.max_tokens_per_run", "400"),
            Fact::labelled("spent", "500"),
        ]);
        assert_eq!(
            evidence.one_line(),
            "limits.max_tokens_per_run: 400; spent: 500"
        );
        assert_eq!(
            evidence.lines(),
            vec!["limits.max_tokens_per_run: 400", "spent: 500"]
        );
    }

    #[test]
    fn evidence_with_nothing_attached_shows_nothing() {
        assert!(Evidence::none().is_empty());
        assert_eq!(Evidence::none().one_line(), "");
    }

    #[test]
    fn a_log_that_recorded_evidence_as_one_string_still_reads() {
        let from_an_older_log: Evidence = serde_json::from_str(r#""exit 1""#).unwrap();
        assert_eq!(from_an_older_log, Evidence::Prose("exit 1".to_string()));
        assert_eq!(from_an_older_log.one_line(), "exit 1");
        assert_eq!(from_an_older_log.lines(), vec!["exit 1"]);
    }

    #[test]
    fn facts_round_trip_through_the_wire_as_a_list() {
        let evidence = Evidence::from(vec![
            Fact::labelled("assignee", "the release lead"),
            Fact::bare("exit 1"),
        ]);
        let json = serde_json::to_string(&evidence).unwrap();
        assert_eq!(
            json,
            r#"[{"label":"assignee","value":"the release lead"},{"value":"exit 1"}]"#
        );
        assert_eq!(
            serde_json::from_str::<Evidence>(&json).unwrap(),
            evidence,
            "what this binary writes reads back as what it wrote"
        );
    }
}
