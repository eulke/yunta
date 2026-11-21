//! What a review found, as the person deciding reads it: how many of each
//! severity, then each finding, the most severe first — what is wrong,
//! where, and in the reviewer's own words what goes wrong and when. The
//! run's findings read as a run's screen lists them: how many were answered
//! and which nodes posted them, a finding a row in the order they were
//! posted, each answered one with its answer under it; then each one with
//! the node that found it, what it says and what other nodes answered.

use surco::components::{Fields, FindingList, FindingRow, Prose, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use surco::look::Look;
use yunta_core::events::findings::{RunFindings, Settled, StandingFinding};
use yunta_core::events::{Finding, FindingSeverity};
use yunta_core::HeldFindings;

use crate::found;
use crate::page::Page;

/// What a review found, as the page the person deciding reads: the findings
/// one document holds, or the run's findings with what was answered.
#[derive(Debug, Clone, Copy)]
pub enum Findings<'a> {
    /// The findings `held`, with `of` after the title: the node or document
    /// they are of (` of `review``), or nothing.
    Held { held: &'a HeldFindings, of: &'a str },
    /// The run's findings, each with the node that found it and what other
    /// nodes answered.
    Run(&'a RunFindings),
}

impl Page for Findings<'_> {
    fn document(&self, look: &Look) -> Doc {
        match self {
            Findings::Held { held, of } => held_document(held, of),
            Findings::Run(view) => run_document(view, look.glyphs),
        }
    }
}

/// The heading of the run's findings `view`, as a run's screen says it: how many were
/// answered of how many were posted, then the nodes that posted them, in the order each
/// first posted one.
pub fn heading(view: &RunFindings, glyphs: Glyphs) -> String {
    let answered = view
        .findings
        .iter()
        .filter(|standing| !standing.answers.is_empty())
        .count();
    let mut posted_by: Vec<&str> = Vec::new();
    for node in view
        .findings
        .iter()
        .filter_map(|standing| standing.node.as_ref())
    {
        if !posted_by.contains(&node.as_str()) {
            posted_by.push(node.as_str());
        }
    }
    let sep = glyphs.sep();
    let mut said = format!(
        "findings {sep} {answered} of {} answered",
        view.findings.len()
    );
    if !posted_by.is_empty() {
        said.push_str(&format!(" {sep} {}", posted_by.join(", ")));
    }
    said
}

/// The run's findings `view` as a run's screen lists them: a finding a row, in the order
/// they were posted, each answered one with its latest answer under it.
pub fn list(view: &RunFindings) -> FindingList {
    FindingList::new(view.findings.iter().map(row).collect())
}

/// One finding as its row: the mark and the word of its severity, its title, and its
/// latest answer once a node answered it.
fn row(standing: &StandingFinding) -> FindingRow {
    let finding = &standing.finding;
    let (title, _) = found::said(&finding.says);
    let row = FindingRow::new(mark(finding.severity), finding.severity.as_str(), title);
    match standing.answers.last() {
        Some(given) => row.answered(format!("{}: {}", given.answer.as_str(), given.why)),
        None => row,
    }
}

/// The findings `held` holds, the most severe first.
fn held_document(held: &HeldFindings, of: &str) -> Doc {
    let mut sorted: Vec<Finding> = held.findings.iter().cloned().map(Finding::from).collect();
    sorted.sort_by_key(|finding| finding.severity);
    let counts = counted(sorted.iter().map(|finding| finding.severity));
    let mut doc = Doc::new().with(Block::Title(surco::components::Title::named(heading_of(
        &format!("findings{of}"),
        &counts,
    ))));
    for finding in sorted {
        let (title, detail) = found::said(&finding.says);
        doc = doc.with(Section {
            lead: Some(surco::components::Lead::Mark(mark(finding.severity))),
            title: titled(finding.severity, &title),
            blocks: vec![
                Prose(format!("{}, at {}", finding.id, finding.location)).into(),
                Block::Markdown(detail),
            ],
        });
    }
    doc
}

/// The run's findings as a run's screen lists them, under its heading, which a page
/// follows with how many of each severity it holds and how many were settled; then each
/// finding in the order it was posted, with the node that found it and what other nodes
/// answered.
fn run_document(view: &RunFindings, glyphs: Glyphs) -> Doc {
    let mut severities: Vec<FindingSeverity> = view
        .findings
        .iter()
        .map(|standing| standing.finding.severity)
        .collect();
    severities.sort();
    let mut said = vec![heading(view, glyphs)];
    said.extend((!severities.is_empty()).then(|| counted(severities.into_iter()).join(", ")));
    let settled = view
        .findings
        .iter()
        .filter(|standing| standing.settled.is_some())
        .count();
    said.extend((settled > 0).then(|| format!("{settled} settled")));
    let title = Line::new().push(Tone::Strong, said.join(&format!(" {} ", glyphs.sep())));
    let mut doc = Doc::new()
        .with(Block::Title(surco::components::Title::named(title)))
        .with(Block::Findings(list(view)));
    for standing in &view.findings {
        doc = doc.with(standing_section(standing, glyphs));
    }
    doc
}

/// What a document of findings is, and how many of each severity it
/// holds.
fn heading_of(what: &str, counts: &[String]) -> Line {
    let said = match counts.is_empty() {
        true => ": none".to_string(),
        false => format!(": {}", counts.join(", ")),
    };
    Line::new().push(Tone::Strong, what).plain(said)
}

fn standing_section(standing: &StandingFinding, glyphs: Glyphs) -> Section {
    let finding = &standing.finding;
    let (title, detail) = found::said(&finding.says);
    let found_by = match &standing.node {
        Some(node) => format!("{} of `{node}`, at {}", finding.id, finding.location),
        None => format!("{}, the run's own, at {}", finding.id, finding.location),
    };
    let mut fields = Fields::new();
    for answer in &standing.answers {
        let by = answer
            .by
            .as_ref()
            .map(|node| format!(" {} by `{node}`", glyphs.sep()))
            .unwrap_or_default();
        fields = fields.push_if(
            "answered",
            format!("{}: {}{by}", answer.answer.as_str(), answer.why),
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
        lead: Some(surco::components::Lead::Mark(mark(finding.severity))),
        title: titled(finding.severity, &title),
        blocks: vec![
            Prose(found_by).into(),
            Block::Markdown(detail),
            fields.into(),
        ],
    }
}

/// A finding's title, after the mark and the word of its severity.
fn titled(severity: FindingSeverity, title: &str) -> Line {
    let mark = mark(severity);
    Line::new()
        .push(Tone::of(mark), severity.as_str())
        .plain(format!(": {title}"))
}

/// The mark a severity is drawn with: a cross for what blocks, caution
/// for what matters, and the quiet mark for the rest.
pub(crate) fn mark(severity: FindingSeverity) -> Mark {
    match severity {
        FindingSeverity::Blocking => Mark::Failed,
        FindingSeverity::Major => Mark::Caution,
        FindingSeverity::Minor | FindingSeverity::Note => Mark::Absent,
    }
}

fn counted(severities: impl Iterator<Item = FindingSeverity>) -> Vec<String> {
    let severities: Vec<FindingSeverity> = severities.collect();
    severities
        .chunk_by(|a, b| a == b)
        .filter_map(|same| Some(format!("{} {}", same.len(), same.first()?.as_str())))
        .collect()
}

#[cfg(test)]
mod tests;
