//! The receipt's two renderings: the PR-facing markdown a person reads,
//! and the JSON a program does.
//!
//! Every number here is already on the [`Receipt`] — this module chooses
//! wording and order, never derives a fact. Keeping it apart from the
//! derivation is what makes "the receipt IS the evidence" checkable: one
//! file reads the log, this one reads the receipt.

use super::{BaselineSummary, DiagnosticCount, EventChainStatus, Receipt, RunnerUsage};
use yunta_core::events::BaselineOrigin;
use yunta_core::text::counted;
use yunta_core::units::Tokens;
use yunta_core::NodeId;

/// Fan-out siblings share their base id (`<base>@<runner>`, see
/// `manifest.rs`) — grouped here purely for the markdown's "reviewed by
/// N independent runners" line; the JSON receipt exposes the flat
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

/// The PR-attachable markdown.
pub fn render_markdown(receipt: &Receipt) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Verified Work Receipt — run {}\n\n",
        receipt.run_id
    ));
    out.push_str(&format!(
        "workflow: `{}` · mode: `{}` · state: {:?}\n\n",
        receipt.workflow, receipt.mode, receipt.terminal_state
    ));

    out.push_str(&format!(
        "- {} {}/{} criteria green (commands + exit codes below)\n",
        mark(receipt.criteria.green == receipt.criteria.total && receipt.criteria.total > 0),
        receipt.criteria.green,
        receipt.criteria.total
    ));
    if !receipt.unknown_kinds.is_empty() {
        let kinds: Vec<String> = receipt
            .unknown_kinds
            .iter()
            .map(|count| format!("`{}` ×{}", count.kind, count.events))
            .collect();
        out.push_str(&format!(
            "- {} {} this binary does not know — interpreted partially: {}\n",
            mark(false),
            counted(receipt.unknown_kinds.len(), "event kind"),
            kinds.join(", ")
        ));
    }
    if !receipt.diagnostics.is_empty() {
        let kinds: Vec<String> = receipt
            .diagnostics
            .iter()
            .map(DiagnosticCount::to_string)
            .collect();
        out.push_str(&format!(
            "- {} {} reported during the run: {}\n",
            mark(false),
            counted(
                receipt.diagnostics.iter().map(|d| d.occurrences).sum(),
                "artifact problem"
            ),
            kinds.join(", ")
        ));
    }
    match &receipt.baseline {
        Some(b) => out.push_str(&baseline_line(b)),
        None => out.push_str("- baseline: not used by this workflow\n"),
    }
    out.push_str(&format!(
        "- {} scope: {} touched, {}\n",
        mark(receipt.scope.violations.is_empty()),
        counted(receipt.scope.files_touched, "file"),
        counted(receipt.scope.violations.len(), "violation")
    ));
    let groups = fan_out_groups(&receipt.runners);
    if groups.is_empty() {
        out.push_str(&format!(
            "- {} used, no fan-out review\n",
            counted(receipt.runners.len(), "runner")
        ));
    } else {
        for (base, members) in &groups {
            let adapters: Vec<&str> = members.iter().map(|m| m.adapter.as_str()).collect();
            out.push_str(&format!(
                "- ✓ Reviewed by {} via `{base}` ({})\n",
                counted(members.len(), "independent runner"),
                adapters.join(", ")
            ));
        }
    }
    out.push_str(&format!(
        "- cost: {} ({} in / {} out){} · {}\n",
        Tokens(receipt.cost.tokens.total()),
        Tokens(receipt.cost.tokens.input).figure(),
        Tokens(receipt.cost.tokens.output).figure(),
        match receipt.cost.cptv {
            Some(cptv) => format!(" · CPTV: {} per task", Tokens::rounded(cptv)),
            None => String::new(),
        },
        counted(receipt.cost.reroutes, "reroute"),
    ));
    match &receipt.event_chain {
        EventChainStatus::Intact { events } => out.push_str(&format!(
            "- ✓ event chain: {}, hash-linked, replayable\n",
            counted(*events, "event")
        )),
        EventChainStatus::Broken { seq, detail } => out.push_str(&format!(
            "{}\n",
            yunta_core::text::detailed(format!("- ✗ event chain BROKEN at seq {seq}"), detail)
        )),
    }

    if !receipt.criteria.entries.is_empty() {
        out.push_str("\n## Criteria\n\n");
        for entry in &receipt.criteria.entries {
            out.push_str(&format!(
                "- {} `{}` — `{}` (exit {})\n",
                mark(entry.exit_code == 0),
                entry.task_id,
                entry.cmd,
                entry.exit_code
            ));
        }
    }
    if !receipt.scope.violations.is_empty() {
        out.push_str("\n## Scope violations\n\n");
        for path in &receipt.scope.violations {
            out.push_str(&format!("- {}\n", path.display()));
        }
    }

    out
}

/// What the run's comparisons against its baseline found — or, when the
/// suite was already failing when it was measured, that none of them
/// could find anything.
fn baseline_line(b: &BaselineSummary) -> String {
    let measured_by = match &b.origin {
        BaselineOrigin::Measured => String::new(),
        BaselineOrigin::Inherited { run } => format!(", measured by run {run}"),
    };
    let hash = b.hash.as_str().get(..12).unwrap_or_default();
    match b.red {
        Some(exit_code) => format!(
            "- {} baseline was already red when measured (exit {exit_code}): {} could find a \
             regression (suite `{}`, hash `{hash}`{measured_by})\n",
            mark(false),
            match b.compared {
                1 => "the one comparison never".to_string(),
                n => format!("none of {n} comparisons"),
            },
            b.suite,
        ),
        None => format!(
            "- {} {} vs baseline across {} (suite `{}`, hash `{hash}`{measured_by})\n",
            mark(b.regressions == 0),
            counted(b.regressions, "regression"),
            counted(b.compared, "comparison"),
            b.suite,
        ),
    }
}

fn mark(ok: bool) -> &'static str {
    if ok {
        "✓"
    } else {
        "✗"
    }
}

/// JSON rendering — the machine-consumable format: same data as the
/// markdown, no separate derivation.
pub fn render_json(receipt: &Receipt) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(receipt)
}
