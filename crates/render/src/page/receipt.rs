//! The receipt as a person reads it — in a file, a pull request or a
//! terminal — and the rows a run's screen says it in: the same rows, a
//! printed receipt keeping after each what only a page has room for, as
//! the page a screen opens over the run keeps it too.
//!
//! Every number here is already on the run's receipt — this page chooses
//! wording and order, never derives a fact. Keeping it apart from the
//! derivation is what makes "the receipt IS the evidence" checkable: the
//! engine reads the log, this reads the receipt.

use surco::components::{Checklist, Fields, Found, Gist, Headline, Next, Section, Whole};
use surco::doc::{Block, Depth, Doc};
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::text::counted;
use surco::words::Standing;
use yunta_core::events::TerminalState;
use yunta_core::receipt::{self, CriteriaSummary, EventChainStatus, FailedNode, FindingsSummary};
use yunta_core::step::NextStep;
use yunta_core::RunId;

use crate::blocks::FailureDetail;
use crate::page::Page;

mod rows;

use rows::{
    baseline, caution_checks, changed, cost, criteria_check, event_chain_check, review_checks,
    said, scope_check,
};

/// The receipt of a run, as the page a person reads it on.
#[derive(Debug, Clone, Copy)]
pub struct Receipt<'a> {
    pub receipt: &'a receipt::Receipt,
}

/// Where a receipt's rows are read, in the glyphs it draws with, which says how much each
/// tells: a run's screen says what each found in the words its pane has room for, and a
/// printed receipt keeps after them, past the separator, the detail only a page has room
/// for, setting a command as a printed page sets one; a page a screen opens over the run
/// says the screen's words and keeps the detail after them.
#[derive(Debug, Clone, Copy)]
pub enum Told {
    OnScreen(Glyphs),
    Printed(Glyphs),
    OnAPage(Glyphs),
}

impl Told {
    /// The glyphs the rows are drawn with.
    fn glyphs(self) -> Glyphs {
        match self {
            Told::OnScreen(glyphs) | Told::Printed(glyphs) | Told::OnAPage(glyphs) => glyphs,
        }
    }
}

impl Page for Receipt<'_> {
    /// What the receipt says, as a document any surface draws: a file writes
    /// it as Markdown, a terminal lays it out. `look`'s glyphs draw the
    /// separators between the facts a line states.
    fn document(&self, look: &Look) -> Doc {
        document(self.receipt, Told::Printed(look.glyphs))
    }
}

impl Receipt<'_> {
    /// The one line a run's screen says the receipt in, under its ribbon, when its work
    /// holds: every criterion green, every finding answered and the log's chain intact.
    /// `None` when any of them does not hold, or no criterion proved the work.
    pub fn gist(&self) -> Option<String> {
        let receipt = self.receipt;
        let criteria = &receipt.criteria;
        let findings = &receipt.findings;
        let holds = criteria.total > 0
            && criteria.green == criteria.total
            && findings.answered == findings.posted
            && matches!(receipt.event_chain, EventChainStatus::Intact { .. });
        holds.then(|| {
            "every criterion green, every finding answered, the log's chain intact".to_string()
        })
    }

    /// One check per thing the run held its work to, in the order a reader meets them, as
    /// `told` says them.
    pub fn checks(&self, told: Told) -> Checklist {
        let receipt = self.receipt;
        let mut checks = Checklist::default();
        criteria_check(&mut checks, receipt, told);
        caution_checks(&mut checks, receipt);
        let (found, words) = baseline(receipt.baseline.as_ref(), told);
        checks.push(found, "baseline", words);
        scope_check(&mut checks, receipt, told);
        review_checks(&mut checks, receipt, told);
        event_chain_check(&mut checks, receipt, told);
        checks
    }

    /// The receipt as a page a screen opens over its run shows it, in `glyphs`: its rows in
    /// the words the run's screen says them, the detail a printed receipt keeps after each,
    /// each criterion's command on its row, cut where it is wider, and no next step, which
    /// the screen's palette offers. Not [`Receipt::document`](Page::document), the receipt
    /// a file, a pull request or a terminal prints.
    pub fn on_screen(&self, glyphs: Glyphs) -> Doc {
        document(self.receipt, Told::OnAPage(glyphs))
    }

    /// What the run spent, what it changed and the pull request it opened, under its
    /// checks, as `told` says them.
    pub fn facts(&self, told: Told) -> Fields {
        let receipt = self.receipt;
        let (spent, detail) = cost(receipt);
        Fields::new()
            .push_if("cost", said(spent, detail, told))
            .push_if("changed", changed(receipt, told))
            .push_if(
                "pull request",
                receipt
                    .pull_request
                    .as_ref()
                    .map_or(String::new(), |opened| {
                        crate::outcome::pull_request(opened.number, &opened.url)
                    }),
            )
    }
}

/// The findings and the criteria in one line, as a gate that ships the work says them above
/// its card before the run closes, in `glyphs`: how many `findings` stand and how many were
/// answered, then how many tasks a check judged and how many of their `criteria` are green.
/// It opens on the done mark where every finding is answered and every criterion green, and
/// on the caution mark otherwise, for the person to look before choosing. Not a receipt's
/// rows, which say each on a row of its own.
pub fn verdicts(findings: &FindingsSummary, criteria: &CriteriaSummary, glyphs: Glyphs) -> Line {
    let green = match criteria.proved == criteria.proving {
        true => format!("{} green", criteria_counted(criteria.proved)),
        false => format!(
            "{} of {} green",
            criteria.proved,
            criteria_counted(criteria.proving)
        ),
    };
    let said = format!(
        "{}, {} answered {} {}, {green}",
        counted(findings.posted, "finding"),
        findings.answered,
        glyphs.sep(),
        counted(criteria.tasks, "task"),
    );
    let holds = findings.answered == findings.posted && criteria.proved == criteria.proving;
    let mark = match holds {
        true => Found::Holds,
        false => Found::Caution,
    }
    .mark();
    Line::new()
        .push(Tone::of(mark), glyphs.mark(mark).to_string())
        .plain(" ")
        .plain(said)
}

/// `count` criteria, in the noun's own plural.
fn criteria_counted(count: usize) -> String {
    match count {
        1 => "1 criterion".to_string(),
        _ => format!("{count} criteria"),
    }
}

/// What the receipt says, as a document any surface draws: a file writes
/// it as Markdown, a terminal lays it out.
///
/// The run's word heads it, then what it is a receipt of and the run, then
/// one check per thing the run held its work to — a cross only for what
/// failed, a `·` for what was never asked for — then what it spent, changed
/// and opened, then each criterion and each path outside the scope, and, on
/// a printed receipt, the command that checks it all again. Each row says
/// what a run's screen says, then what only a page keeps, as `told` says it;
/// its glyphs draw the separators between the facts a line states.
fn document(receipt: &receipt::Receipt, told: Told) -> Doc {
    let handle = receipt.run_id.handle();
    // A run still open is read as its last step begins: it is running,
    // and its receipt says so before anything it found.
    let word = receipt.terminal_state.map_or(Standing::Running, |state| {
        standing(state, receipt.blocking_findings)
    });
    let headline = Headline::new(
        word.mark(),
        format!("receipt of {} ({})", receipt.workflow, receipt.mode),
        word.word().to_string(),
    )
    .edge(handle);
    let mut doc = Doc::new().with(headline);
    if receipt.terminal_state.is_none() {
        doc = doc.with(Gist::new("the run is still open: this is its last step"));
    }
    let page = Receipt { receipt };
    let mut doc = doc
        .with(Fields::new().push_if("run", receipt.run_id.to_string()))
        .with(page.checks(told))
        .with(page.facts(told));
    for failed in &receipt.failed {
        doc = doc.with(failed_node(failed, &receipt.run_id));
    }
    if let Some(criteria) = criteria(receipt, told) {
        doc = doc
            .with(Block::Heading("criteria".to_string()))
            .with(criteria);
    }
    if let Some(outside) = outside(receipt) {
        doc = doc
            .with(Block::Heading("outside the scope".to_string()))
            .with(outside);
    }
    let Told::Printed(_) = told else {
        return doc;
    };
    let verify = NextStep::Verify {
        run: receipt.run_id.clone(),
    };
    doc.with(Next {
        steps: vec![(crate::command::line(&verify), crate::command::does(&verify))],
    })
}

/// A node that stands failed: its id and the sentence its failure is
/// told in, then what it printed and the command that shows the rest.
fn failed_node(failed: &FailedNode, run: &RunId) -> Section {
    let node = &failed.node;
    Section {
        lead: Some(surco::components::Lead::Mark(Mark::Failed)),
        title: Line::new()
            .push(Tone::Strong, format!("node `{node}`"))
            .plain(": ")
            .plain(crate::failure::headline(&failed.failure)),
        blocks: FailureDetail {
            failure: &failed.failure,
            whole: Some(Whole::Command(crate::command::line(&NextStep::Status {
                run: run.clone(),
                node: Some(node.clone()),
                depth: Depth::Glance,
            }))),
        }
        .blocks(),
    }
}

/// Each criterion the run ran, by the task it judged, when it ran any: on a page a screen
/// opens, each command on its row, cut where it is wider, as a cut command still names
/// itself.
fn criteria(receipt: &receipt::Receipt, told: Told) -> Option<Checklist> {
    if receipt.criteria.entries.is_empty() {
        return None;
    }
    let mut criteria = match told {
        Told::OnAPage(_) => Checklist::default().on_their_rows(),
        Told::OnScreen(_) | Told::Printed(_) => Checklist::default(),
    };
    for entry in &receipt.criteria.entries {
        let found = match entry.exit_code {
            0 => Found::Holds,
            _ => Found::Problem,
        };
        criteria.push(
            found,
            entry.task_id.to_string(),
            format!("`{}` exits {}", entry.cmd, entry.exit_code),
        );
    }
    Some(criteria)
}

/// Each path the run wrote outside its scope, when it wrote any.
fn outside(receipt: &receipt::Receipt) -> Option<Checklist> {
    if receipt.scope.violations.is_empty() {
        return None;
    }
    let mut outside = Checklist::default();
    for path in &receipt.scope.violations {
        outside.push(Found::Problem, path.display().to_string(), "");
    }
    Some(outside)
}

/// The word a run that closed this way is called by: one that finished
/// holding findings that block it is reported, as every surface says it.
fn standing(state: TerminalState, blocking_findings: usize) -> Standing {
    match state {
        TerminalState::Done if blocking_findings > 0 => Standing::Reported,
        TerminalState::Done => Standing::Finished,
        TerminalState::Failed => Standing::Failed,
        TerminalState::Cancelled => Standing::Stopped,
        TerminalState::Promoted => Standing::Promoted,
    }
}

#[cfg(test)]
mod tests;
