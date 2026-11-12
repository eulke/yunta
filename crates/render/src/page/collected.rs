//! What a pass of `gc` did, said as one page once the pass is over: a
//! row for each thing it took away, emptied or kept, and what the
//! project's checkouts hold after it.

use super::Page;
use crate::command;
use surco::components::{Fields, Furrow, Gist, Headline, Next};
use surco::doc::Doc;
use surco::figures::Bytes;
use surco::glyphs::Mark;
use surco::look::Look;
use surco::text::{counted, counted_as};
use yunta_core::step::NextStep;
use yunta_core::RunId;

/// What a pass did, row by row as it went, and how much it could not do: the
/// page it says once it is over.
#[derive(Debug, Clone)]
pub struct Collected {
    /// Whether the pass only says what it would do.
    pub dry_run: bool,
    /// Whether no retention is configured, so no run is collected.
    pub unretained: bool,
    /// How many runs went whole, or would.
    pub reclaimed: usize,
    /// What was done, or would be, in the order it was.
    rows: Vec<Reclaim>,
    /// What the project's checkouts keep after the pass.
    pub now: Option<String>,
    /// How many things the pass could not do, each warned as it happened.
    pub problems: usize,
}

impl Collected {
    pub fn new(dry_run: bool) -> Self {
        Collected {
            dry_run,
            unretained: false,
            reclaimed: 0,
            rows: Vec::new(),
            now: None,
            problems: 0,
        }
    }

    /// A row of the page: what the pass did, or would.
    pub fn did(&mut self, reclaim: Reclaim) {
        self.rows.push(reclaim);
    }
}

/// One thing a pass of `gc` did, or under a dry run would do: a row of its
/// page, said in the tense the pass is in. A path is held as a reader is
/// shown it.
#[derive(Debug, Clone, PartialEq)]
pub enum Reclaim {
    /// A run's own checkout or its `run.dir`, taken away.
    Removed { at: String },
    /// This many events of `run`, purged from the log once its files went.
    Purged { run: RunId, events: usize },
    /// The work of this many units' branches, which runs made before refs
    /// kept it, kept by refs.
    UnitWorkKept { branches: usize },
    /// The build in the project's checkout at `checkout`, emptied, and what
    /// it took.
    BuildEmptied { checkout: String, bytes: u64 },
    /// A checkout of the project's pool nothing used, taken away.
    UnusedCheckoutRemoved { checkout: String },
    /// A clean checkout `run` kept before the project's pool, taken away.
    OldCheckoutRemoved { checkout: String, run: RunId },
    /// A checkout `run` kept before the project's pool, kept because
    /// something in it is uncommitted.
    OldCheckoutKept { checkout: String, run: RunId },
}

impl Reclaim {
    /// The row: what was done, in the tense `dry_run` puts it in, and to
    /// what. Only removing, purging and keeping the units' work is ever a
    /// dry run's.
    fn row(&self, dry_run: bool) -> (&'static str, String) {
        let tense = |done, would| match dry_run {
            true => would,
            false => done,
        };
        match self {
            Reclaim::Removed { at } => (tense("removed", "would remove"), at.clone()),
            Reclaim::Purged { run, events } => (
                tense("purged", "would purge"),
                format!("{} of run {}", counted(*events, "event"), run.handle()),
            ),
            Reclaim::UnitWorkKept { branches } => (
                tense("kept", "would keep"),
                format!(
                    "the work of {} by refs",
                    counted_as(*branches, "unit branch", "unit branches")
                ),
            ),
            Reclaim::BuildEmptied { checkout, bytes } => (
                "emptied",
                format!("the build of {checkout} ({})", Bytes(*bytes)),
            ),
            Reclaim::UnusedCheckoutRemoved { checkout } => {
                ("removed", format!("the unused checkout {checkout}"))
            }
            Reclaim::OldCheckoutRemoved { checkout, run } => {
                ("removed", format!("{checkout} of run {}", run.handle()))
            }
            Reclaim::OldCheckoutKept { checkout, run } => (
                "kept",
                format!(
                    "{checkout} of run {}: something in it is uncommitted",
                    run.handle()
                ),
            ),
        }
    }
}

/// Something a pass of `gc` could not do, left for a later pass, said as it
/// happens: what it was about, and the failure in its own words.
#[derive(Debug, Clone, PartialEq)]
pub enum Uncollected {
    /// Something of run `run`: its log, its manifest, or what git holds of it.
    Run { run: RunId, failed: String },
    /// The units' branches runs made before refs kept their work.
    UnitBranches { failed: String },
    /// The project's checkouts.
    Checkouts { failed: String },
    /// A checkout the project's pool could not say it owns, left where it is.
    LeftAlone { checkout: String, failed: String },
    /// A run's checkout or directory that would not go.
    Unremoved { at: String, failed: String },
}

impl Uncollected {
    /// What a person watching the pass reads, as it happens.
    pub fn said(&self) -> String {
        match self {
            Uncollected::Run { run, failed } => format!("run {}: {failed}", run.handle()),
            Uncollected::UnitBranches { failed } => format!("the units' branches: {failed}"),
            Uncollected::Checkouts { failed } => format!("the project's checkouts: {failed}"),
            Uncollected::LeftAlone { checkout, failed } => {
                format!("left {checkout} alone: {failed}")
            }
            Uncollected::Unremoved { at, failed } => format!("failed to remove {at}: {failed}"),
        }
    }
}

impl Page for Collected {
    /// The page: what the pass reclaimed, each row it did, and the pass
    /// that does what a dry run found.
    fn document(&self, _look: &Look) -> Doc {
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
        let mut headline =
            Headline::new(mark, command::line(&NextStep::Gc { dry_run: false }), said);
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
            .map(|reclaim| reclaim.row(self.dry_run))
            .fold(Fields::new(), |rows, (done, what)| rows.push_if(done, what))
            .push_if("now", self.now.clone().unwrap_or_default());
        let steps = match self.dry_run && !self.rows.is_empty() {
            true => vec![(
                command::line(&NextStep::Gc { dry_run: false }),
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
    use surco::look::Look;
    use surco::surface::{Surface, Terminal};

    use yunta_core::RunId;

    use super::{Collected, Reclaim, Uncollected};
    use crate::page::Page;

    #[test]
    fn a_pass_says_what_it_reclaimed_then_a_row_for_each_thing_it_did() {
        let run = RunId::from_static("01K3W48MFW7H0ZZA5PZ0ABCDEF");
        let mut collected = Collected::new(false);
        collected.reclaimed = 1;
        let removed = |at: &str| Reclaim::Removed { at: at.to_string() };
        collected.did(removed("~/.yunta/worktrees/01K3W48MFW7H0ZZA5PZ07E5PH4"));
        collected.did(removed("~/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4"));
        collected.did(Reclaim::Purged {
            run: run.clone(),
            events: 12,
        });
        collected.now = Some("1 checkout, builds 0 B of a 10 GB budget; 40 GB free".to_string());
        let furrow = ".".repeat(80);
        assert_eq!(
            Terminal::on(Look::plain()).draw(&collected.document(&Look::plain())),
            format!(
                "+ yunta gc ; reclaimed 1 run\n{furrow}\n\n  \
                 removed      ~/.yunta/worktrees/01K3W48MFW7H0ZZA5PZ07E5PH4\n               \
                 ~/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4\n  \
                 purged       12 events of run {}\n  \
                 now          1 checkout, builds 0 B of a 10 GB budget; 40 GB free\n",
                run.handle()
            )
        );
    }

    #[test]
    fn the_pool_rows_name_what_each_checkout_held() {
        let run = RunId::from_static("01K3W48MFW7H0ZZA5PZ0ABCDEF");
        let rows = [
            Reclaim::UnitWorkKept { branches: 2 },
            Reclaim::BuildEmptied {
                checkout: "~/.yunta/pool/a".to_string(),
                bytes: 0,
            },
            Reclaim::UnusedCheckoutRemoved {
                checkout: "~/.yunta/pool/b".to_string(),
            },
            Reclaim::OldCheckoutKept {
                checkout: "~/.yunta/units/c".to_string(),
                run: run.clone(),
            },
        ];
        let said: Vec<_> = rows.iter().map(|row| row.row(false)).collect();
        assert_eq!(
            said[0],
            ("kept", "the work of 2 unit branches by refs".to_string())
        );
        assert_eq!(
            said[1],
            ("emptied", "the build of ~/.yunta/pool/a (0 B)".to_string())
        );
        assert_eq!(
            said[2],
            ("removed", "the unused checkout ~/.yunta/pool/b".to_string())
        );
        assert_eq!(
            said[3],
            (
                "kept",
                format!(
                    "~/.yunta/units/c of run {}: something in it is uncommitted",
                    run.handle()
                )
            )
        );
        assert_eq!(rows[0].row(true).0, "would keep");
    }

    #[test]
    fn what_a_pass_could_not_do_names_what_it_was_about() {
        let run = RunId::from_static("01K3W48MFW7H0ZZA5PZ0ABCDEF");
        let failed = || "denied".to_string();
        assert_eq!(
            Uncollected::Run {
                run: run.clone(),
                failed: failed()
            }
            .said(),
            format!("run {}: denied", run.handle())
        );
        assert_eq!(
            Uncollected::LeftAlone {
                checkout: "~/w".to_string(),
                failed: failed()
            }
            .said(),
            "left ~/w alone: denied"
        );
        assert_eq!(
            Uncollected::Unremoved {
                at: "~/w".to_string(),
                failed: failed()
            }
            .said(),
            "failed to remove ~/w: denied"
        );
    }

    #[test]
    fn a_dry_run_says_what_it_would_do_and_the_pass_that_does_it() {
        let mut collected = Collected::new(true);
        collected.reclaimed = 1;
        collected.did(Reclaim::Removed {
            at: "~/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4".to_string(),
        });
        let page = Terminal::on(Look::plain()).draw(&collected.document(&Look::plain()));
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
        let mut collected = Collected::new(true);
        collected.now = Some("1 checkout, builds 0 B of a 10 GB budget; 40 GB free".to_string());
        let page = Terminal::on(Look::plain()).draw(&collected.document(&Look::plain()));
        assert!(
            page.starts_with(". yunta gc ; would reclaim no run\n"),
            "{page}"
        );
        assert!(!page.contains("next"), "{page}");
    }

    #[test]
    fn a_pass_with_no_retention_says_why_it_collects_no_run() {
        let mut collected = Collected::new(false);
        collected.unretained = true;
        // A problem is warned where the pass meets it; the page counts it.
        collected.problems += 1;
        let page = Terminal::on(Look::plain()).draw(&collected.document(&Look::plain()));
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
