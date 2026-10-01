//! What a review found, as the person deciding reads it on a terminal:
//! how many of each severity, then each finding, the most severe first —
//! what is wrong, where, and in the reviewer's own words what goes wrong
//! and when. The run's findings read the same way, each with the node
//! that found it and what other nodes answered.

use yunta_core::events::findings::{RunFindings, StandingFinding};
use yunta_core::events::FindingSeverity;
use yunta_core::{FindingEntry, FindingsFile};

use crate::render::markdown::{hanging, markdown};
use crate::render::INDENT;

/// Where a finding's facts sit: one step under its headline.
const BODY: &str = "    ";

pub(super) fn findings(file: &FindingsFile, of: &str, width: usize) -> Vec<String> {
    let mut found: Vec<&FindingEntry> = file.findings.iter().collect();
    found.sort_by_key(|finding| finding.severity);
    let counts = counted(found.iter().map(|finding| finding.severity));
    let mut lines = vec![match counts.is_empty() {
        true => format!("the findings{of} — none"),
        false => format!("the findings{of} — {}", counts.join(", ")),
    }];
    for finding in found {
        lines.push(String::new());
        lines.extend(headline(finding.severity, &finding.title, width));
        lines.extend(hanging(
            BODY,
            "",
            &format!("{}, at {}", finding.id, finding.location),
            width,
        ));
        lines.extend(markdown(&finding.detail, BODY, width));
    }
    lines
}

/// Every finding standing in the run: how many of each severity and how
/// many another node answered, then each, the most severe first — which
/// node found it where, what goes wrong, and each answer it got.
pub(super) fn run_findings(view: &RunFindings, width: usize) -> Vec<String> {
    let mut found: Vec<&StandingFinding> = view.findings.iter().collect();
    found.sort_by_key(|standing| standing.finding.severity);
    let counts = counted(found.iter().map(|standing| standing.finding.severity));
    let answered = found
        .iter()
        .filter(|standing| !standing.answers.is_empty())
        .count();
    let settled = found
        .iter()
        .filter(|standing| standing.settled.is_some())
        .count();
    let mut lines = vec![match counts.is_empty() {
        true => "the run's findings — none".to_string(),
        false => format!(
            "the run's findings — {}{}",
            counts.join(", "),
            tally(answered, settled)
        ),
    }];
    for standing in found {
        lines.push(String::new());
        lines.extend(standing_lines(standing, width));
    }
    lines
}

/// One standing finding: what is wrong, which node found it where, what
/// goes wrong, each answer it got and what its proof showed.
fn standing_lines(standing: &StandingFinding, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let finding = &standing.finding;
    let found_by = match &standing.node {
        Some(node) => format!("{} of `{node}`, at {}", finding.id, finding.location),
        None => format!("{}, the run's own, at {}", finding.id, finding.location),
    };
    lines.extend(headline(finding.severity, &finding.title, width));
    lines.extend(hanging(BODY, "", &found_by, width));
    lines.extend(markdown(&finding.detail, BODY, width));
    for answer in &standing.answers {
        let by = answer
            .by
            .as_ref()
            .map(|node| format!(" by `{node}`"))
            .unwrap_or_default();
        lines.extend(hanging(
            BODY,
            &format!("{}{by} — ", answer.answer.as_str()),
            &answer.why,
            width,
        ));
    }
    if let Some(proof) = &standing.proof {
        let verdict = match standing.settled.is_some() {
            true => "settled — ",
            false => "not proved — ",
        };
        let said = format!("`{}` exits {}", proof.cmd, proof.exit_code);
        lines.extend(hanging(BODY, verdict, &said, width));
    }
    lines
}

/// What of the run's findings others answered and what settled, after
/// the severities; empty when neither.
fn tally(answered: usize, settled: usize) -> String {
    let said: Vec<String> = [(answered, "answered"), (settled, "settled")]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, what)| format!("{count} {what}"))
        .collect();
    match said.is_empty() {
        true => String::new(),
        false => format!("; {}", said.join(", ")),
    }
}

/// A finding's first line: how severe, and what is wrong.
fn headline(severity: FindingSeverity, title: &str, width: usize) -> Vec<String> {
    hanging(INDENT, &format!("{} — ", severity.as_str()), title, width)
}

/// How many of each severity `severities` holds, the most severe first;
/// `severities` comes sorted.
fn counted(severities: impl Iterator<Item = FindingSeverity>) -> Vec<String> {
    let severities: Vec<FindingSeverity> = severities.collect();
    severities
        .chunk_by(|a, b| a == b)
        .filter_map(|same| Some(format!("{} {}", same.len(), same.first()?.as_str())))
        .collect()
}
