//! What a pass of `gc` did, said as one page once the pass is over: a
//! row for each thing it took away, emptied or kept, and what the
//! project's checkouts hold after it.

use surco::components::{Fields, Furrow, Gist, Headline, Next};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::text::counted;
use yunta_core::step::NextStep;
use yunta_render::command;

use crate::error::warn;

/// What a pass did, row by row as it went, and how much it could not do.
pub(super) struct Report {
    /// Whether the pass only says what it would do.
    pub(super) dry_run: bool,
    /// Whether no retention is configured, so no run is collected.
    pub(super) unretained: bool,
    /// How many runs went whole, or would.
    pub(super) reclaimed: usize,
    /// What was done, or would be, and to what, in the order it was.
    rows: Vec<(&'static str, String)>,
    /// What the project's checkouts keep after the pass.
    pub(super) now: Option<String>,
    /// How many things the pass could not do, each warned as it happened.
    problems: usize,
}

impl Report {
    pub(super) fn new(dry_run: bool) -> Self {
        Report {
            dry_run,
            unretained: false,
            reclaimed: 0,
            rows: Vec::new(),
            now: None,
            problems: 0,
        }
    }

    /// A row of the page: `done` to `what`.
    pub(super) fn did(&mut self, done: &'static str, what: impl Into<String>) {
        self.rows.push((done, what.into()));
    }

    /// Something the pass could not do, warned as it happens and counted
    /// on the page.
    pub(super) fn problem(&mut self, said: impl std::fmt::Display) {
        warn(said);
        self.problems += 1;
    }

    /// The page: what the pass reclaimed, each row it did, and the pass
    /// that does what a dry run found.
    pub(super) fn page(&self) -> Doc {
        let said = match (self.reclaimed, self.dry_run) {
            (0, false) => "no run to reclaim".to_string(),
            (0, true) => "would reclaim no run".to_string(),
            (runs, true) => format!("would reclaim {}", counted(runs, "run")),
            (runs, false) => format!("reclaimed {}", counted(runs, "run")),
        };
        let mark = match (self.problems, self.dry_run) {
            (0, false) => Mark::Done,
            (0, true) => Mark::Absent,
            _ => Mark::Caution,
        };
        let mut headline = Headline::new(mark, command::line(&NextStep::Gc), said);
        if self.problems > 0 {
            headline = headline.fact(format!("{} above", counted(self.problems, "problem")));
        }
        let unretained = self.unretained.then(|| {
            Gist::new(
                "`storage.retention_days` isn't configured, so no run is collected: `gc` has no \
                 default retention to guess",
            )
            .into()
        });
        let rows = self
            .rows
            .iter()
            .fold(Fields::new(), |rows, (done, what)| {
                rows.push_if(done, what.clone())
            })
            .push_if("now", self.now.clone().unwrap_or_default());
        let steps = match self.dry_run && !self.rows.is_empty() {
            true => vec![(
                yunta_render::command::line(&NextStep::Gc),
                "does what this pass would",
            )],
            false => Vec::new(),
        };
        Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with_all(unretained)
            .with(rows)
            .with(Next { steps })
    }
}

#[cfg(test)]
mod tests {
    use super::Report;
    use crate::output::draw;
    use surco::look::Look;

    #[test]
    fn a_pass_says_what_it_reclaimed_then_a_row_for_each_thing_it_did() {
        let mut report = Report::new(false);
        report.reclaimed = 1;
        report.did("removed", "~/.yunta/worktrees/01K3W48MFW7H0ZZA5PZ07E5PH4");
        report.did("removed", "~/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4");
        report.did("purged", "12 events of run ABCDEF");
        report.now = Some("1 checkout, builds 0 B of a 10 GB budget; 40 GB free".to_string());
        let furrow = ".".repeat(80);
        assert_eq!(
            draw(report.page(), &Look::plain()),
            format!(
                "+ yunta gc ; reclaimed 1 run\n{furrow}\n\n  \
                 removed      ~/.yunta/worktrees/01K3W48MFW7H0ZZA5PZ07E5PH4\n               \
                 ~/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4\n  \
                 purged       12 events of run ABCDEF\n  \
                 now          1 checkout, builds 0 B of a 10 GB budget; 40 GB free\n"
            )
        );
    }

    #[test]
    fn a_dry_run_says_what_it_would_do_and_the_pass_that_does_it() {
        let mut report = Report::new(true);
        report.reclaimed = 1;
        report.did("would remove", "~/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4");
        let page = draw(report.page(), &Look::plain());
        assert!(
            page.starts_with(". yunta gc ; would reclaim 1 run\n"),
            "{page}"
        );
        assert!(
            page.ends_with("\nnext\n  yunta gc   does what this pass would\n"),
            "{page}"
        );
    }

    #[test]
    fn a_dry_run_that_finds_nothing_offers_no_pass() {
        let mut report = Report::new(true);
        report.now = Some("1 checkout, builds 0 B of a 10 GB budget; 40 GB free".to_string());
        let page = draw(report.page(), &Look::plain());
        assert!(
            page.starts_with(". yunta gc ; would reclaim no run\n"),
            "{page}"
        );
        assert!(!page.contains("next"), "{page}");
    }

    #[test]
    fn a_pass_with_no_retention_says_why_it_collects_no_run() {
        let mut report = Report::new(false);
        report.unretained = true;
        report.problem("the project's checkouts: no disk");
        let page = draw(report.page(), &Look::plain());
        assert!(
            page.starts_with("^ yunta gc ; no run to reclaim ; 1 problem above\n"),
            "{page}"
        );
        assert!(
            page.contains("  `storage.retention_days` isn't configured, so no run is collected:"),
            "{page}"
        );
    }
}
