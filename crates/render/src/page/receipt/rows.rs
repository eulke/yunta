//! See [`super`]. The receipt's rows: one check per thing the run held its work to, and
//! what it spent and changed, each in the words a run's screen says it and, on a printed
//! receipt, with the detail only a page keeps after the separator.

use surco::components::{Checklist, Found};
use surco::figures::Tokens;
use surco::text::counted;
use yunta_core::events::BaselineOrigin;
use yunta_core::receipt::{self, BaselineSummary, DiagnosticCount, EventChainStatus};

use super::Told;
use crate::spend::Breakdown;

/// `words`, and on a printed receipt `detail` after the separator, where there is one.
pub(super) fn said(words: String, detail: Option<String>, told: Told) -> String {
    match (told, detail) {
        (Told::Printed(glyphs), Some(detail)) => format!("{words} {} {detail}", glyphs.sep()),
        _ => words,
    }
}

/// The run's criteria: the tasks' own, how many are green and whether each was red before
/// the work, then on a printed receipt their guards apart; none declared is not applicable.
pub(super) fn criteria_check(checks: &mut Checklist, receipt: &receipt::Receipt, told: Told) {
    let criteria = &receipt.criteria;
    if criteria.total == 0 {
        checks.push(
            Found::NotApplicable,
            "criteria",
            "none declared, nothing to prove",
        );
        return;
    }
    let found = match criteria.green == criteria.total {
        true => Found::Holds,
        false => Found::Problem,
    };
    let guards = (criteria.guards > 0).then(|| match criteria.guards_green == criteria.guards {
        true => format!("{} green", counted(criteria.guards, "guard")),
        false => format!(
            "{} of {} green",
            criteria.guards_green,
            counted(criteria.guards, "guard")
        ),
    });
    let (words, detail) = match criteria.proving {
        0 => (guards.unwrap_or_default(), None),
        proving => {
            let green = format!("{} of {proving} green", criteria.proved);
            let words = match criteria.red_before == proving {
                true => format!("{green}, red before the work"),
                false => green,
            };
            (words, guards)
        }
    };
    checks.push(found, "criteria", said(words, detail, told));
}

/// The event kinds this binary reads only in part, and the artifact problems
/// reported during the run, when there are any.
pub(super) fn caution_checks(checks: &mut Checklist, receipt: &receipt::Receipt) {
    if let Some(said) = crate::state::unknown_kinds(&receipt.unknown_kinds) {
        checks.push(Found::Caution, "event kinds", said);
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
}

/// The files the run touched and how many of them a person granted, then on a printed
/// receipt how many it wrote outside its scope; with none granted, both on every one.
pub(super) fn scope_check(checks: &mut Checklist, receipt: &receipt::Receipt, told: Told) {
    let scope = &receipt.scope;
    let found = match scope.violations.is_empty() {
        true => Found::Holds,
        false => Found::Problem,
    };
    let files = counted(scope.files_touched, "file");
    let outside = match scope.violations.len() {
        0 => "none outside it".to_string(),
        n => format!("{n} outside it"),
    };
    let words = match (scope.files_touched, scope.granted) {
        (0, _) => "no file touched".to_string(),
        (_, 0) => format!("{files} touched, {outside}"),
        (_, granted) => said(
            format!("{files}, {granted} of them granted by you"),
            Some(outside),
            told,
        ),
    };
    checks.push(found, "scope", words);
}

/// The review of the run's work: how many findings stand and how many were answered, then
/// on a printed receipt who reviewed it. A run with no finding says who reviewed it alone:
/// one check per group of fan-out siblings, or a note that no runner fanned out.
pub(super) fn review_checks(checks: &mut Checklist, receipt: &receipt::Receipt, told: Told) {
    let reviewers = reviewers(receipt);
    let findings = &receipt.findings;
    if findings.posted == 0 {
        for (found, said) in reviewers {
            checks.push(found, "review", said);
        }
        return;
    }
    let found = match (
        receipt.blocking_findings,
        findings.answered == findings.posted,
    ) {
        (0, true) => Found::Holds,
        (0, false) => Found::Caution,
        _ => Found::Problem,
    };
    let words = format!(
        "{}, {} answered",
        counted(findings.posted, "finding"),
        findings.answered
    );
    let who: Vec<String> = reviewers.into_iter().map(|(_, said)| said).collect();
    checks.push(found, "review", said(words, Some(who.join("; ")), told));
}

/// Who reviewed the run's work: each group of fan-out siblings, or that no runner fanned
/// out, each with how its check stands.
fn reviewers(receipt: &receipt::Receipt) -> Vec<(Found, String)> {
    let groups = receipt.fan_out_groups();
    if groups.is_empty() {
        let used = format!(
            "{} used, no fan-out review",
            counted(receipt.runners.len(), "runner")
        );
        return vec![(Found::NotApplicable, used)];
    }
    groups
        .iter()
        .map(|(base, members)| {
            let adapters: Vec<&str> = members.iter().map(|m| m.adapter.as_str()).collect();
            let by = format!(
                "by {} via `{base}` ({})",
                counted(members.len(), "independent runner"),
                adapters.join(", ")
            );
            (Found::Holds, by)
        })
        .collect()
}

/// Whether the run's events form one hash-linked chain, then on a printed receipt how
/// many; or where it breaks.
pub(super) fn event_chain_check(checks: &mut Checklist, receipt: &receipt::Receipt, told: Told) {
    match &receipt.event_chain {
        EventChainStatus::Intact { events } => {
            let detail = format!("{}, hash-linked, replayable", counted(*events, "event"));
            checks.push(
                Found::Holds,
                "event chain",
                said("intact".to_string(), Some(detail), told),
            );
        }
        EventChainStatus::Broken { seq, detail } => checks.push(
            Found::Problem,
            "event chain",
            surco::text::detailed(format!("broken at seq {seq}"), detail),
        ),
    }
}

/// What the run spent: per verified task once the run had tasks, else every token, and
/// the reroutes it took; then what a printed receipt keeps, every token in and out.
pub(super) fn cost(receipt: &receipt::Receipt) -> (String, Option<String>) {
    let cost = &receipt.cost;
    if cost.tokens.total() == 0 && cost.reroutes == 0 {
        return ("nothing spent".to_string(), None);
    }
    let reroutes = counted(cost.reroutes, "reroute");
    let total = Tokens(cost.tokens.total());
    let breakdown = Breakdown(&cost.tokens);
    match cost.cptv {
        Some(cptv) => (
            format!("{} per verified task, {reroutes}", Tokens::rounded(cptv)),
            Some(format!("{total} ({breakdown})")),
        ),
        None => (format!("{total}, {reroutes}"), Some(breakdown.to_string())),
    }
}

/// What the run changed from its base to its newest tree, as git last
/// counted it; nothing when nothing counted it, or it touched no file.
pub(super) fn changed(receipt: &receipt::Receipt, told: Told) -> String {
    receipt
        .changed
        .as_ref()
        .and_then(|change| crate::changes::of(change, told.glyphs()))
        .unwrap_or_default()
}

/// What the run's comparisons against its baseline found, as `told` says it: that the
/// suite still passes, a printed receipt setting its command as a printed page sets one and
/// keeping after it what the comparisons found — or, when the suite was already failing
/// when it was measured, that none of them could find anything.
pub(super) fn baseline(baseline: Option<&BaselineSummary>, told: Told) -> (Found, String) {
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
    let compared = format!(
        "{} across {} (suite `{}`, hash `{hash}`{measured_by})",
        counted(b.regressions, "regression"),
        counted(b.compared, "comparison"),
        b.suite,
    );
    match (b.red, b.regressions) {
        (Some(exit_code), _) => (
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
        (None, 0) => {
            let passes = match told {
                Told::OnScreen(_) => format!("{} still passes", b.suite),
                Told::Printed(_) => format!("`{}` still passes", b.suite),
            };
            (Found::Holds, said(passes, Some(compared), told))
        }
        (None, _) => (Found::Problem, compared),
    }
}
