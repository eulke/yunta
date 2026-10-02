//! `yunta stats --workflow <name>`: a workflow's history — its cost per
//! verified task over time, its modes side by side, and what its checks
//! never caught.

use yunta_core::{ModeName, WorkflowName};
use yunta_engine::{prior_estimation, RunSummary};

use super::format_estimation_line;
use crate::render::{cell_width, sparkline, truncate, Look, Tokens, INDENT, LABEL_WIDTH};

/// The whole `yunta stats --workflow <name>` block, ready to print.
pub(super) fn render_workflow_history(
    workflow_name: &WorkflowName,
    history: &[RunSummary],
    look: Look,
) -> String {
    let mut out = format!(
        "workflow `{workflow_name}` — {}\n",
        yunta_core::text::counted(history.len(), "run")
    );

    out.push_str("\nCPTV over time:\n");
    out.push_str(&format!("{}\n", cptv_line(history, look)));

    out.push_str("\nmodes:\n");
    for (mode, runs, median_cptv, median_tokens) in mode_table(history) {
        out.push_str(&format!(
            "{INDENT}{} {:>3} {:<4}   median CPTV {}   median tokens {}\n",
            truncate(mode.as_str(), LABEL_WIDTH, look.glyphs),
            runs,
            yunta_core::text::agreeing(runs, "run", "runs"),
            median_cptv
                .map(|v| Tokens::rounded(v).figure())
                .unwrap_or_else(|| "n/a".to_string()),
            median_tokens
                .map(|v| Tokens::rounded(v).figure())
                .unwrap_or_else(|| "n/a".to_string()),
        ));
    }

    match prior_estimation(history) {
        Some(estimation) => {
            out.push_str(&format!(
                "\n{}\n",
                format_estimation_line(&estimation, look.glyphs)
            ));
        }
        None => out.push_str(&format!(
            "\nestimation: not enough runs yet (need {}, have {})\n",
            yunta_engine::MIN_SAMPLES_FOR_ESTIMATION,
            history.len()
        )),
    }
    out
}

/// Every past run's CPTV as one cell, oldest first, with the newest
/// value spelled out beside it — the sparkline carries the shape and the
/// number carries the scale.
///
/// The sparkline gets whatever `width` leaves after the indent and that
/// note, so a workflow with hundreds of runs narrows its window
/// instead of wrapping the line and breaking the block it sits in.
fn cptv_line(history: &[RunSummary], Look { glyphs, width, .. }: Look) -> String {
    let latest = history
        .last()
        .and_then(|r| r.cptv)
        .map(|c| format!("{c:.1}"))
        .unwrap_or_else(|| "n/a".to_string());
    let note = format!("  (oldest {} newest, latest = {latest})", glyphs.arrow());
    let cells = width
        .cells()
        .saturating_sub(cell_width(INDENT) + cell_width(&note));
    let series: Vec<f64> = history.iter().map(|r| r.cptv.unwrap_or(0.0)).collect();
    format!("{INDENT}{}{note}", sparkline(&series, cells, glyphs))
}

/// Verification-effectiveness findings — advisory only, never a reason
/// `stats` or `check` exits non-zero: these are suggestions for a
/// person to weigh,
/// not errors. Returns the rendered text (empty if there's nothing to
/// say) so each caller can send it to stdout (`stats`) or stderr
/// (`check`, alongside its own warnings) without duplicating the
/// wording.
pub(crate) fn render_verification_findings(
    findings: &yunta_engine::VerificationFindings,
) -> String {
    if findings.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("verification performance — advisory, nothing here is acted on automatically:\n");
    for c in &findings.never_red_criteria {
        out.push_str(&format!(
            "{INDENT}criterion `{}` was never red in pre-check across {} — \
             either redundant, or mis-written (both readings shown, never just one)\n",
            c.cmd,
            yunta_core::text::counted(c.sample_count, "run")
        ));
    }
    for r in &findings.never_triggered_reroutes {
        out.push_str(&format!(
            "{INDENT}node `{}`'s re-route to `{}` never fired across {} — \
             the prior flow is more reliable than expected\n",
            r.node,
            r.goto,
            yunta_core::text::counted(r.sample_count, "run")
        ));
    }
    for g in &findings.always_approved_gates {
        out.push_str(&format!(
            "{INDENT}gate `{}` was approved without adjustment across {} — \
             still adding value, or become ritual?\n",
            g.node,
            yunta_core::text::counted(g.sample_count, "resolution")
        ));
    }
    if let Some(t) = &findings.always_first_try_tasks {
        out.push_str(&format!(
            "{INDENT}every task passed on its first try across {} — \
             the plan may be cutting too fine\n",
            yunta_core::text::counted(t.sample_count, "task instance")
        ));
    }
    for m in &findings.unused_modes {
        out.push_str(&format!(
            "{INDENT}mode `{}` was never chosen across {} — \
             still worth declaring?\n",
            m.name,
            yunta_core::text::counted(m.runs_observed, "run")
        ));
    }
    out
}

/// Median CPTV/tokens per mode — a plain historical comparison, not a
/// prediction, so it doesn't gate on
/// [`yunta_engine::MIN_SAMPLES_FOR_ESTIMATION`] the way
/// [`prior_estimation`] does: a mode with one run still gets a row, just
/// with that run's own numbers as its "median".
fn mode_table(history: &[RunSummary]) -> Vec<(ModeName, usize, Option<f64>, Option<f64>)> {
    let mut modes: Vec<ModeName> = history.iter().map(|r| r.mode.clone()).collect();
    modes.sort();
    modes.dedup();

    modes
        .into_iter()
        .map(|mode| {
            let runs: Vec<&RunSummary> = history.iter().filter(|r| r.mode == mode).collect();
            let mut cptv: Vec<f64> = runs.iter().filter_map(|r| r.cptv).collect();
            let mut tokens: Vec<f64> = runs.iter().map(|r| r.tokens as f64).collect();
            cptv.sort_by(|a, b| a.total_cmp(b));
            tokens.sort_by(|a, b| a.total_cmp(b));
            (
                mode,
                runs.len(),
                yunta_engine::median(&cptv),
                yunta_engine::median(&tokens),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use yunta_core::{ContentHash, RunId};

    use super::*;
    use crate::render::Glyphs;

    fn summary(cptv: f64) -> RunSummary {
        RunSummary {
            run_id: RunId::from_static("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
            mode: ModeName::default(),
            workflow_hash: ContentHash::sha256(b"workflow"),
            tokens: 1000,
            wall_clock: None,
            tasks_total: 1,
            cptv: Some(cptv),
        }
    }

    #[test]
    fn a_long_history_narrows_its_sparkline_instead_of_wrapping_the_line() {
        let history: Vec<RunSummary> = (1..=200).map(|n| summary(f64::from(n))).collect();
        let line = cptv_line(&history, Look::plain());
        let cells = cell_width(&line);
        assert!(
            cells <= Look::plain().width.cells(),
            "{cells} cells: {line}"
        );
        assert!(
            line.contains(Glyphs::Ascii.ellipsis()),
            "a narrowed window says so: {line}"
        );
    }

    #[test]
    fn a_history_that_fits_draws_every_run_and_marks_no_window() {
        let history: Vec<RunSummary> = (1..=3).map(|n| summary(f64::from(n))).collect();
        let line = cptv_line(&history, Look::plain());
        assert!(!line.contains(Glyphs::Ascii.ellipsis()), "{line}");
        assert!(line.contains("latest = 3.0"), "{line}");
    }
}
