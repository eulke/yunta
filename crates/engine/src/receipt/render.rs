//! The receipt's two renderings: the PR-facing markdown a person reads,
//! and the JSON a program does.
//!
//! Every number here is already on the [`Receipt`] — this module chooses
//! wording and order, never derives a fact. Keeping it apart from the
//! derivation is what makes "the receipt IS the evidence" checkable: one
//! file reads the log, this one reads the receipt.

use super::{DiagnosticCount, EventChainStatus, Receipt, RunnerUsage};
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
            "- {} event kind(s) this binary does not know — interpreted partially: {}\n",
            mark(false),
            kinds.join(", ")
        ));
    }
    if !receipt.diagnostics.is_empty() {
        let counted: Vec<String> = receipt
            .diagnostics
            .iter()
            .map(DiagnosticCount::to_string)
            .collect();
        out.push_str(&format!(
            "- {} document problem(s) reported during the run: {}\n",
            mark(false),
            counted.join(", ")
        ));
    }
    let checks = &receipt.self_checks;
    if checks.checks > 0 {
        let corrected: Vec<String> = checks
            .corrected
            .iter()
            .map(DiagnosticCount::to_string)
            .collect();
        out.push_str(&format!(
            "- {} {} self-check(s) before close, {} clean{}\n",
            mark(checks.nodes_that_never_checked.is_empty()),
            checks.checks,
            checks.clean,
            if corrected.is_empty() {
                String::new()
            } else {
                format!(" — corrected in place: {}", corrected.join(", "))
            }
        ));
    }
    if !checks.nodes_that_never_checked.is_empty() {
        let nodes: Vec<String> = checks
            .nodes_that_never_checked
            .iter()
            .map(|node| format!("`{node}`"))
            .collect();
        out.push_str(&format!(
            "- {} node(s) produced an interpreted artifact without checking it: {}\n",
            mark(false),
            nodes.join(", ")
        ));
    }
    match &receipt.baseline {
        Some(b) => out.push_str(&format!(
            "- {} {} regression(s) vs baseline across {} comparison(s) (suite `{}`, hash `{}`)\n",
            mark(b.regressions == 0),
            b.regressions,
            b.compared,
            b.suite,
            b.hash.as_str().get(..12).unwrap_or_default()
        )),
        None => out.push_str("- baseline: not used by this workflow\n"),
    }
    out.push_str(&format!(
        "- {} scope: {} file(s) touched, {} violation(s)\n",
        mark(receipt.scope.violations.is_empty()),
        receipt.scope.files_touched,
        receipt.scope.violations.len()
    ));
    let groups = fan_out_groups(&receipt.runners);
    if groups.is_empty() {
        out.push_str(&format!(
            "- {} runner(s) used, no fan-out review\n",
            receipt.runners.len()
        ));
    } else {
        for (base, members) in &groups {
            let adapters: Vec<&str> = members.iter().map(|m| m.adapter.as_str()).collect();
            out.push_str(&format!(
                "- ✓ Reviewed by {} independent runner(s) via `{base}` ({})\n",
                members.len(),
                adapters.join(", ")
            ));
        }
    }
    out.push_str(&format!(
        "- cost: {} tokens ({} in / {} out){} · {} reroute(s)\n",
        receipt.cost.tokens.total(),
        receipt.cost.tokens.input,
        receipt.cost.tokens.output,
        match receipt.cost.cptv {
            Some(cptv) => format!(" · CPTV: {cptv:.1} tokens/task"),
            None => String::new(),
        },
        receipt.cost.reroutes,
    ));
    match &receipt.event_chain {
        EventChainStatus::Intact { events } => out.push_str(&format!(
            "- ✓ event chain: {events} event(s), hash-linked, replayable\n"
        )),
        EventChainStatus::Broken { seq, detail } => {
            out.push_str(&format!("- ✗ event chain BROKEN at seq {seq}: {detail}\n"))
        }
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
