//! `yunta stats --workflow <name>`: a workflow's history — its cost per
//! verified task over time, its modes side by side, and what its checks
//! never caught — told as a run's numbers are, one page printed and on the
//! application's screen: a headline that names the workflow and counts its
//! runs, what they usually take as its sentence, the cost per verified task as
//! a labelled row, and a mode a row as who spent a run's tokens is drawn, a bar
//! of its median task cost against the dearest.

use super::barred::barred;
use super::run::TASK_COST;
use crate::page::Page;
use surco::bars::sparkline;
use surco::components::{Checklist, Found, Furrow, Gist, Headline};
use surco::doc::{Block, Doc};
use surco::figures::Tokens;
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::text::counted;
use surco::width::{cell_width, INDENT, LABEL_WIDTH};
use yunta_core::{ModeName, WorkflowName};
use yunta_replay::history::{PriorEstimation, RunSummary};
use yunta_replay::verification_effectiveness::VerificationFindings;

/// What heads the advice a workflow's history gives about its checks.
const CHECKS_PERFORMED: &str = "how its checks performed";

/// What the advice is said to be beside its heading: never acted on.
const ADVICE_ONLY: &str = ": advice, nothing here is acted on";

/// What heads a workflow's modes on its screen.
const BY_MODE: &str = "by mode";

/// What the `yunta stats --workflow <name>` page is drawn from: a
/// workflow's past runs, and what their logs say about its checks.
pub struct History<'a> {
    pub name: &'a WorkflowName,
    pub history: &'a [RunSummary],
    pub findings: Option<&'a VerificationFindings>,
}

impl Page for History<'_> {
    /// The page, printed and on a screen alike, as a run's numbers are told: a headline that
    /// names the workflow and counts its runs, what they usually take under it, its cost per
    /// verified task over time as a labelled row, `by mode` and a mode a row with a bar of its
    /// median task cost against the dearest, then what its checks never caught, under their
    /// heading. A workflow nobody has run is told so in a line of its own.
    fn document(&self, look: &Look) -> Doc {
        if self.history.is_empty() {
            return Doc::new().with(Block::Lines(vec![
                Line::new().plain(format!("no runs of workflow `{}` yet", self.name))
            ]));
        }
        let runs = counted(self.history.len(), "run");
        let headline = Headline::new(Mark::Absent, self.name.as_str(), runs);
        let usually = match PriorEstimation::of(self.history) {
            Some(estimation) => [
                crate::spend::usual_tokens(&estimation),
                crate::spend::usual_wall_clock(&estimation),
            ]
            .join(&look.glyphs.spaced_sep()),
            None => too_few_runs(self.history),
        };
        let modes = mode_table(self.history)
            .into_iter()
            .map(|(mode, runs, cptv, tokens)| {
                let figure = [
                    counted(runs, "run"),
                    format!("median task cost {}", figure_of(cptv)),
                    format!("median tokens {}", figure_of(tokens)),
                ]
                .join(&look.glyphs.spaced_sep());
                (mode.to_string(), cptv.unwrap_or_default(), figure)
            });
        let mut doc = Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with(Gist::new(usually))
            .with(Block::Rows(vec![task_cost(self.history, *look)]))
            .with(Block::Heading(BY_MODE.to_string()))
            .with(Block::Rows(barred(modes.collect(), look)));
        if let Some(findings) = self.findings.filter(|findings| !findings.is_empty()) {
            let heading = format!("{CHECKS_PERFORMED}{ADVICE_ONLY}");
            doc = doc.with(Block::Heading(heading)).with(advice(findings));
        }
        doc
    }
}

/// The cost per verified task over time as a row of a page's facts, one row cut where it
/// ends: its label muted in the column a page's facts stand in, every past run's cost a cell
/// of a sparkline in the running tone, oldest first, then what it is and the latest's figure,
/// `█▁▂  per verified task, oldest → newest; the latest 2.17M`. Where the row has no room
/// for a cell a run and what it says, the sparkline narrows to its newest runs, which it
/// says, down to [`LEAST_SPARKLINE`] cells.
fn task_cost(history: &[RunSummary], Look { glyphs, width, .. }: Look) -> Line {
    let series: Vec<f64> = history.iter().map(|r| r.cptv.unwrap_or(0.0)).collect();
    let latest = history.last().and_then(|r| r.cptv);
    let said = format!(
        "  per verified task, oldest {} newest; the latest {}",
        glyphs.arrow(),
        figure_of(latest)
    );
    let label = cell_width(INDENT) + LABEL_WIDTH + 1;
    let room = width.cells().saturating_sub(label + cell_width(&said));
    let cells = series.len().min(room.max(LEAST_SPARKLINE));
    Line::new()
        .plain(INDENT)
        .push(Tone::Muted, format!("{TASK_COST:<LABEL_WIDTH$}"))
        .plain(" ")
        .push(Tone::Running, sparkline(&series, cells, glyphs))
        .plain(said)
}

/// The fewest cells the sparkline of a workflow's cost per verified task keeps however narrow
/// its row: a short history stays whole and what the row says after it is cut instead; a
/// longer one narrows to its newest runs only down to this.
const LEAST_SPARKLINE: usize = 8;

/// A median or a cost as its figure, rounded: `n/a` where there is none.
fn figure_of(value: Option<f64>) -> String {
    value
        .map(|v| Tokens::rounded(v).figure())
        .unwrap_or_else(|| "n/a".to_string())
}

/// What a history too short for an estimate says in its place: how many runs it needs, and
/// how many stand.
fn too_few_runs(history: &[RunSummary]) -> String {
    format!(
        "estimation: not enough runs yet (need {}, have {})",
        yunta_replay::history::MIN_SAMPLES,
        history.len()
    )
}

/// How effective a workflow's checks were across its runs, as its history
/// says — advisory only, never a reason `stats` or `check` exits non-zero:
/// suggestions for a person to weigh, not errors, each a caution row. A caller
/// with no findings to say says nothing at all.
impl Page for VerificationFindings {
    fn document(&self, _look: &Look) -> Doc {
        Doc::new()
            .with(Block::Title(surco::components::Title::named(
                Line::new()
                    .push(Tone::Strong, CHECKS_PERFORMED)
                    .push(Tone::Muted, ADVICE_ONLY),
            )))
            .with(advice(self))
    }
}

/// One caution row per thing a workflow's history says about its checks.
fn advice(findings: &VerificationFindings) -> Checklist {
    let runs = |n| surco::text::counted(n, "run");
    let mut checks = Checklist::default();
    for c in &findings.never_red_criteria {
        checks.push(
            Found::Caution,
            format!("criterion `{}`", c.cmd),
            format!(
                "never red in pre-check across {}: either redundant, or mis-written",
                runs(c.sample_count)
            ),
        );
    }
    for r in &findings.never_triggered_reroutes {
        checks.push(
            Found::Caution,
            format!("node `{}`", r.node),
            format!(
                "its reroute to `{}` never fired across {}: the flow before it is more \
                 reliable than expected",
                r.goto,
                runs(r.sample_count)
            ),
        );
    }
    habits(&mut checks, findings);
    checks
}

/// What the history says about a workflow's gates, tasks and modes.
fn habits(
    checks: &mut Checklist,
    findings: &yunta_replay::verification_effectiveness::VerificationFindings,
) {
    for g in &findings.always_approved_gates {
        checks.push(
            Found::Caution,
            format!("gate `{}`", g.node),
            format!(
                "approved without adjustment across {}: still adding value, or become ritual?",
                surco::text::counted(g.sample_count, "resolution")
            ),
        );
    }
    if let Some(t) = &findings.always_first_try_tasks {
        checks.push(
            Found::Caution,
            "every task",
            format!(
                "passed on its first try across {}: the plan may be cutting too fine",
                surco::text::counted(t.sample_count, "task instance")
            ),
        );
    }
    for m in &findings.unused_modes {
        checks.push(
            Found::Caution,
            format!("mode `{}`", m.name),
            format!(
                "never chosen across {}: still worth declaring?",
                surco::text::counted(m.runs_observed, "run")
            ),
        );
    }
}

/// Median CPTV/tokens per mode — a plain historical comparison, not a
/// prediction, so it doesn't gate on
/// [`yunta_replay::history::MIN_SAMPLES`] the way
/// [`PriorEstimation::of`] does: a mode with one run still gets a row, just
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
                yunta_replay::stats::median(&cptv),
                yunta_replay::stats::median(&tokens),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use yunta_core::{ContentHash, RunId};

    use super::*;
    use surco::glyphs::Glyphs;

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
    fn a_long_history_narrows_its_sparkline_instead_of_passing_its_row() {
        let history: Vec<RunSummary> = (1..=200).map(|n| summary(f64::from(n))).collect();
        let row = task_cost(&history, Look::plain()).text();
        let cells = cell_width(&row);
        assert!(cells <= Look::plain().width.cells(), "{cells} cells: {row}");
        assert!(
            row.contains(Glyphs::Ascii.ellipsis()),
            "a narrowed window says so: {row}"
        );
        assert!(
            row.ends_with("the latest 200"),
            "what it says stays whole: {row}"
        );
    }

    #[test]
    fn a_history_that_fits_draws_every_run_and_marks_no_window() {
        let history: Vec<RunSummary> = (1..=3).map(|n| summary(f64::from(n))).collect();
        let row = task_cost(&history, Look::plain()).text();
        assert!(!row.contains(Glyphs::Ascii.ellipsis()), "{row}");
        assert!(row.ends_with("the latest 3"), "{row}");
    }
}
