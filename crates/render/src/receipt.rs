//! The receipt as a person reads it — in a file, a pull request or a
//! terminal.
//!
//! Every number here is already on the [`Receipt`] — this module chooses
//! wording and order, never derives a fact. Keeping it apart from the
//! derivation is what makes "the receipt IS the evidence" checkable: the
//! engine reads the log, this reads the receipt.

use yunta_core::events::{BaselineOrigin, TerminalState};
use yunta_core::receipt::{
    BaselineSummary, DiagnosticCount, EventChainStatus, Receipt, RunnerUsage,
};
use yunta_core::text::counted;
use yunta_core::units::Tokens;
use yunta_core::NodeId;

use crate::blocks::{Checklist, Fields, Found, Headline, Next, Prose};
use crate::doc::{Block, Doc};
use crate::RunWord;

/// Fan-out siblings share their base id (`<base>@<runner>`, see
/// `manifest.rs`) — grouped here purely for the receipt's review check; the JSON receipt exposes the flat
/// `runners` list instead and leaves grouping to whoever consumes it.
pub fn fan_out_groups(runners: &[RunnerUsage]) -> Vec<(NodeId, Vec<&RunnerUsage>)> {
    let mut groups: Vec<(NodeId, Vec<&RunnerUsage>)> = Vec::new();
    for usage in runners {
        if !usage.node_id.is_fan_out() {
            continue;
        }
        let base = usage.node_id.base();
        match groups.iter_mut().find(|(b, _)| *b == base) {
            Some((_, members)) => members.push(usage),
            None => groups.push((base, vec![usage])),
        }
    }
    groups.retain(|(_, members)| members.len() > 1);
    groups
}

/// What the receipt says, as a document any surface draws: a file writes
/// it as Markdown, a terminal lays it out.
///
/// The run's word heads it, then what it is a receipt of, then one check
/// per thing the run held its work to — a cross only for what failed, a
/// `·` for what was never asked for — then each criterion and each path
/// outside the scope, and the command that checks it all again.
pub fn document(receipt: &Receipt) -> Doc<'static> {
    let handle = receipt.run_id.handle();
    // A run still open is read as its last step begins: it is running,
    // and its receipt says so before anything it found.
    let word = receipt.terminal_state.map_or(RunWord::Running, run_word);
    let mut doc = Doc::new().with(Headline {
        subject: format!("receipt for run {handle}"),
        mark: word.mark(),
        said: word.word().to_string(),
    });
    if receipt.terminal_state.is_none() {
        doc = doc.with(Prose(
            "the run is still open: this is its last step".to_string(),
        ));
    }
    let mut doc = doc
        .with(
            Fields::new()
                .push_if("workflow", receipt.workflow.to_string())
                .push_if("mode", receipt.mode.to_string())
                .push_if("run", receipt.run_id.to_string())
                .push_if("cost", cost(receipt)),
        )
        .with(checks(receipt));
    if let Some(criteria) = criteria(receipt) {
        doc = doc
            .with(Block::Heading("criteria".to_string()))
            .with(criteria);
    }
    if let Some(outside) = outside(receipt) {
        doc = doc
            .with(Block::Heading("outside the scope".to_string()))
            .with(outside);
    }
    doc.with(Next {
        steps: vec![(
            format!("yunta verify {handle}"),
            "checks this run's evidence again",
        )],
    })
}

/// Each criterion the run ran, by the task it judged, when it ran any.
fn criteria(receipt: &Receipt) -> Option<Checklist> {
    if receipt.criteria.entries.is_empty() {
        return None;
    }
    let mut criteria = Checklist::default();
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
fn outside(receipt: &Receipt) -> Option<Checklist> {
    if receipt.scope.violations.is_empty() {
        return None;
    }
    let mut outside = Checklist::default();
    for path in &receipt.scope.violations {
        outside.push(Found::Problem, path.display().to_string(), "");
    }
    Some(outside)
}

/// The word a run that closed this way is called by.
fn run_word(state: TerminalState) -> RunWord {
    match state {
        TerminalState::Done => RunWord::Finished,
        TerminalState::Failed => RunWord::Failed,
        TerminalState::Cancelled => RunWord::Cancelled,
        TerminalState::Promoted => RunWord::Promoted,
    }
}

/// One check per thing the run held its work to.
fn checks(receipt: &Receipt) -> Checklist {
    let mut checks = Checklist::default();
    let criteria = &receipt.criteria;
    match (criteria.total, criteria.green == criteria.total) {
        (0, _) => checks.push(
            Found::NotApplicable,
            "criteria",
            "none declared, nothing to prove",
        ),
        (total, all) => checks.push(
            if all { Found::Holds } else { Found::Problem },
            "criteria",
            format!("{}/{total} green, each command below", criteria.green),
        ),
    }
    if !receipt.unknown_kinds.is_empty() {
        let kinds: Vec<String> = receipt
            .unknown_kinds
            .iter()
            .map(|count| format!("`{}` ×{}", count.kind, count.events))
            .collect();
        checks.push(
            Found::Caution,
            "event kinds",
            format!(
                "{} this binary does not know — interpreted partially: {}",
                counted(receipt.unknown_kinds.len(), "event kind"),
                kinds.join(", ")
            ),
        );
    }
    if !receipt.diagnostics.is_empty() {
        let kinds: Vec<String> = receipt
            .diagnostics
            .iter()
            .map(DiagnosticCount::to_string)
            .collect();
        checks.push(
            Found::Caution,
            "artifacts",
            format!(
                "{} reported during the run: {}",
                counted(
                    receipt.diagnostics.iter().map(|d| d.occurrences).sum(),
                    "artifact problem"
                ),
                kinds.join(", ")
            ),
        );
    }
    let (found, said) = baseline(receipt.baseline.as_ref());
    checks.push(found, "baseline", said);
    checks.push(
        match receipt.scope.violations.is_empty() {
            true => Found::Holds,
            false => Found::Problem,
        },
        "scope",
        format!(
            "{} touched, {} outside it",
            counted(receipt.scope.files_touched, "file"),
            match receipt.scope.violations.len() {
                0 => "none".to_string(),
                n => n.to_string(),
            }
        ),
    );
    let groups = fan_out_groups(&receipt.runners);
    if groups.is_empty() {
        checks.push(
            Found::NotApplicable,
            "review",
            format!(
                "{} used, no fan-out review",
                counted(receipt.runners.len(), "runner")
            ),
        );
    }
    for (base, members) in &groups {
        let adapters: Vec<&str> = members.iter().map(|m| m.adapter.as_str()).collect();
        checks.push(
            Found::Holds,
            "review",
            format!(
                "by {} via `{base}` ({})",
                counted(members.len(), "independent runner"),
                adapters.join(", ")
            ),
        );
    }
    match &receipt.event_chain {
        EventChainStatus::Intact { events } => checks.push(
            Found::Holds,
            "event chain",
            format!("{}, hash-linked, replayable", counted(*events, "event")),
        ),
        EventChainStatus::Broken { seq, detail } => checks.push(
            Found::Problem,
            "event chain",
            yunta_core::text::detailed(format!("broken at seq {seq}"), detail),
        ),
    }
    checks
}

/// What the run spent: every token, in and out, per task once the run
/// had tasks, and the re-routes it took.
fn cost(receipt: &Receipt) -> String {
    let cost = &receipt.cost;
    if cost.tokens.total() == 0 && cost.reroutes == 0 {
        return "nothing spent".to_string();
    }
    let mut said = format!(
        "{} ({} in / {} out)",
        Tokens(cost.tokens.total()),
        Tokens(cost.tokens.input).figure(),
        Tokens(cost.tokens.output).figure(),
    );
    if let Some(cptv) = cost.cptv {
        said.push_str(&format!(" · CPTV: {} per task", Tokens::rounded(cptv)));
    }
    said.push_str(&format!(" · {}", counted(cost.reroutes, "reroute")));
    said
}

/// What the run's comparisons against its baseline found — or, when the
/// suite was already failing when it was measured, that none of them
/// could find anything.
fn baseline(baseline: Option<&BaselineSummary>) -> (Found, String) {
    let Some(b) = baseline else {
        return (
            Found::NotApplicable,
            "not used by this workflow".to_string(),
        );
    };
    let measured_by = match &b.origin {
        BaselineOrigin::Measured => String::new(),
        BaselineOrigin::Inherited { run } => format!(", measured by run {run}"),
    };
    let hash = b.hash.as_str().get(..12).unwrap_or_default();
    match b.red {
        Some(exit_code) => (
            Found::Caution,
            format!(
                "already red when measured (exit {exit_code}): {} could find a regression \
                 (suite `{}`, hash `{hash}`{measured_by})",
                match b.compared {
                    1 => "the one comparison never".to_string(),
                    n => format!("none of {n} comparisons"),
                },
                b.suite,
            ),
        ),
        None => (
            match b.regressions {
                0 => Found::Holds,
                _ => Found::Problem,
            },
            format!(
                "{} across {} (suite `{}`, hash `{hash}`{measured_by})",
                counted(b.regressions, "regression"),
                counted(b.compared, "comparison"),
                b.suite,
            ),
        ),
    }
}
