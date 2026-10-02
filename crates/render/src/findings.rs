//! What a review found, as the person deciding reads it: how many of each
//! severity, then each finding, the most severe first — what is wrong,
//! where, and in the reviewer's own words what goes wrong and when. The
//! run's findings read the same way, each with the node that found it
//! and what other nodes answered.

use yunta_core::events::findings::{RunFindings, Settled, StandingFinding};
use yunta_core::events::FindingSeverity;
use yunta_core::{FindingEntry, FindingsFile};

use crate::blocks::{Fields, Prose, Section};
use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};
use crate::Mark;

pub fn document(file: &FindingsFile, of: &str) -> Doc<'static> {
    let mut found: Vec<&FindingEntry> = file.findings.iter().collect();
    found.sort_by_key(|finding| finding.severity);
    let counts = counted(found.iter().map(|finding| finding.severity));
    let mut doc = Doc::new().with(Block::Title(heading(
        &format!("findings{of}"),
        &counts,
        String::new(),
    )));
    for finding in found {
        doc = doc.with(Section {
            mark: Some(mark(finding.severity)),
            title: titled(finding.severity, &finding.title),
            blocks: vec![
                Prose(format!("{}, at {}", finding.id, finding.location)).into(),
                Block::Markdown(finding.detail.clone()),
            ],
        });
    }
    doc
}

pub fn run_document(view: &RunFindings) -> Doc<'static> {
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
    let mut doc = Doc::new().with(Block::Title(heading(
        "the run's findings",
        &counts,
        tally(answered, settled),
    )));
    for standing in found {
        doc = doc.with(standing_section(standing));
    }
    doc
}

/// What a document of findings is, and how many of each severity it
/// holds.
fn heading(what: &str, counts: &[String], tally: String) -> Line {
    let said = match counts.is_empty() {
        true => ": none".to_string(),
        false => format!(": {}{tally}", counts.join(", ")),
    };
    Line::new().push(Tone::Strong, what).plain(said)
}

fn standing_section(standing: &StandingFinding) -> Section<'static> {
    let finding = &standing.finding;
    let found_by = match &standing.node {
        Some(node) => format!("{} of `{node}`, at {}", finding.id, finding.location),
        None => format!("{}, the run's own, at {}", finding.id, finding.location),
    };
    let mut fields = Fields::new();
    for answer in &standing.answers {
        let by = answer
            .by
            .as_ref()
            .map(|node| format!(" by `{node}`"))
            .unwrap_or_default();
        fields = fields.push_if(
            "answered",
            format!("{}{by} — {}", answer.answer.as_str(), answer.why),
        );
    }
    if let Some(proof) = &standing.proof {
        let label = match standing.settled {
            Some(Settled::Proof { .. }) => "settled",
            _ => "not proved",
        };
        fields = fields.push_if(label, format!("`{}` exits {}", proof.cmd, proof.exit_code));
    }
    if let Some(Settled::Person { gate }) = &standing.settled {
        fields = fields.push_if("settled", format!("a person went on past `{gate}`"));
    }
    Section {
        mark: Some(mark(finding.severity)),
        title: titled(finding.severity, &finding.title),
        blocks: vec![
            Prose(found_by).into(),
            Block::Markdown(finding.detail.clone()),
            fields.into(),
        ],
    }
}

/// A finding's title, after the mark and the word of its severity.
fn titled(severity: FindingSeverity, title: &str) -> Line {
    let mark = mark(severity);
    Line::new()
        .push(Tone::of(mark), severity.as_str())
        .plain(format!(" — {title}"))
}

/// The mark a severity is drawn with: a cross for what blocks, caution
/// for what matters, and the quiet mark for the rest.
pub fn mark(severity: FindingSeverity) -> Mark {
    match severity {
        FindingSeverity::Blocking => Mark::Failed,
        FindingSeverity::Major => Mark::Caution,
        FindingSeverity::Minor | FindingSeverity::Note => Mark::Pending,
    }
}

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

fn counted(severities: impl Iterator<Item = FindingSeverity>) -> Vec<String> {
    let severities: Vec<FindingSeverity> = severities.collect();
    severities
        .chunk_by(|a, b| a == b)
        .filter_map(|same| Some(format!("{} {}", same.len(), same.first()?.as_str())))
        .collect()
}
