//! `yunta list --runs`: every local run as an inbox — grouped by what a
//! reader can do about it (what stopped on a person, what stalled, what
//! is still moving, what has closed) and ordered inside each group so the
//! run to answer first is the first one read. A run that needs someone
//! says what holds it and the command that moves it, under its row.
//!
//! Which runs it lists, and in which group, is the [`Inbox`]'s answer;
//! this module lays it out.

use yunta_core::text::counted;
use yunta_core::units::DURATION_WIDEST;
use yunta_core::RunId;

use super::inbox::{Inbox, RunRow, Standing, Unreadable};
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::render::ink::{Line, Tone};
use crate::render::state::RunWord;
use crate::render::{cell_width, duration, indent, truncate, wrap, Look, INDENT};

/// The cells a run's handle gets: what a reader copies into the next
/// command, so the column pads a shorter one and never cuts one.
const HANDLE_WIDTH: usize = RunId::HANDLE_CHARS;

/// The cells the workflow name and the run's mode share — enough for the
/// names a repo's own workflows carry, and the column the eye runs down
/// to find the run it came for.
const NAME_WIDTH: usize = 44;

/// How many closed runs the listing names: the newest, which are the ones
/// a reader comes back for. Those past it are counted, so a listing of a
/// busy repository still opens on what needs someone.
const CLOSED_SHOWN: usize = 10;

/// The runs of the repository this is run in — every run on the machine
/// with `all`, or when this is run outside a repository — as the inbox,
/// and how many runs the listing leaves to other projects.
pub async fn list_runs(all: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let inbox = Inbox::gather(&ctx, all).await?;
    if inbox.is_empty() {
        println!("no runs in {}", ctx.project.storage_path.display());
        return Ok(Outcome::Success);
    }
    let Inbox {
        rows,
        unreadable,
        elsewhere,
    } = inbox;
    if rows.is_empty() && unreadable.is_empty() {
        println!("no runs in this repository");
    } else {
        print!(
            "{}",
            render_runs(rows, unreadable, &crate::render::stdout_look())
        );
    }
    if elsewhere > 0 {
        println!(
            "\n{} in other projects — yunta list --runs --all",
            counted(elsewhere, "run")
        );
    }
    Ok(Outcome::Success)
}

/// The listing itself: what needs a person first, then what stalled,
/// then what is still moving, then what has closed, and last the runs
/// that do not read back.
///
/// Within the groups a run is still in, the run that has been there
/// longest comes first — the one that has been waiting since yesterday
/// is the one to answer before the one that stopped a minute ago — and
/// runs that arrived at their state together are ordered by id, which
/// for a ULID is the order they were created in. Closed runs are the
/// other way round, newest first, and only the newest
/// [`CLOSED_SHOWN`] are named.
///
/// A run under `unreadable` has no state to have been in for any length
/// of time, so that group is ordered by id alone.
fn render_runs(mut rows: Vec<RunRow>, mut unreadable: Vec<Unreadable>, look: &Look) -> String {
    rows.sort_by(|a, b| b.age.cmp(&a.age).then_with(|| a.run_id.cmp(&b.run_id)));
    let mut out = String::new();
    for standing in Standing::ALL {
        let mut group: Vec<&RunRow> = rows.iter().filter(|row| row.standing == standing).collect();
        if group.is_empty() {
            continue;
        }
        push_heading(&mut out, standing.heading(), group.len());
        let mut folded = 0;
        if standing == Standing::Closed {
            group.reverse();
            folded = group.len().saturating_sub(CLOSED_SHOWN);
            group.truncate(CLOSED_SHOWN);
        }
        for row in group {
            for line in row.lines(look) {
                out.push_str(&format!("{}\n", look.ink.paint(&line)));
            }
        }
        if folded > 0 {
            out.push_str(&format!(
                "{INDENT}{} not listed\n",
                counted(folded, "older closed run")
            ));
        }
    }
    if !unreadable.is_empty() {
        unreadable.sort_by(|a, b| a.run_id.cmp(&b.run_id));
        push_heading(&mut out, "unreadable", unreadable.len());
        for run in &unreadable {
            out.push_str(&format!(
                "{INDENT}{}: {}\n",
                run.run_id.handle(),
                run.problem
            ));
        }
    }
    out
}

/// A group's heading, with the runs in it counted: a reader who only
/// reads the headings still learns how much is waiting.
fn push_heading(out: &mut String, heading: &str, runs: usize) {
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("{heading} ({runs})\n"));
}

impl RunRow {
    /// The run's row — its word, the handle a reader copies, the
    /// workflow and mode it runs, and how long it has been there — and
    /// under it, when there is one, what holds it said whole and the
    /// command that moves it.
    fn lines(&self, look: &Look) -> Vec<Line> {
        let mut lines = vec![self.head(look)];
        // What holds the run hangs one step further under its row, so it
        // reads as this run's line rather than the next run's.
        let margin = indent(2);
        let under = look.width.cells().saturating_sub(cell_width(&margin));
        if let Some(reason) = &self.reason {
            lines.extend(
                wrap(reason, under)
                    .into_iter()
                    .map(|part| Line::new().plain(margin.as_str()).plain(part)),
            );
        }
        if let Some(command) = &self.command {
            lines.push(
                Line::new()
                    .plain(margin.as_str())
                    .push(Tone::Strong, command.as_str()),
            );
        }
        lines
    }

    /// The row itself, its columns lined up with every other row's.
    fn head(&self, look: &Look) -> Line {
        let glyphs = look.glyphs;
        let mark = self.word.mark();
        let tone = Tone::of(mark);
        // What the row takes besides the name: the margin, the mark and
        // the word, the handle, the age, and the gaps between them.
        let fixed = cell_width(INDENT) + 2 + RunWord::WIDEST + HANDLE_WIDTH + DURATION_WIDEST + 6;
        let room = NAME_WIDTH.min(look.width.cells().saturating_sub(fixed));
        let name = truncate(&format!("{} ({})", self.workflow, self.mode), room, glyphs);
        Line::new()
            .plain(INDENT)
            .push(
                tone,
                format!(
                    "{} {:<width$}",
                    glyphs.mark(mark),
                    self.word.word(),
                    width = RunWord::WIDEST
                ),
            )
            .plain("  ")
            .push(
                Tone::Strong,
                format!("{:<HANDLE_WIDTH$}", self.run_id.handle()),
            )
            .plain("  ")
            .plain(name)
            .plain("  ")
            // The age right-aligned, so a column of them compares as
            // numbers.
            .push(
                Tone::Muted,
                format!("{:>DURATION_WIDEST$}", duration(self.age)),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use yunta_core::{ModeName, RunId};

    use super::*;
    use crate::render::state::RunWord;

    fn row(id: &'static str, standing: Standing, age_secs: u64) -> RunRow {
        let word = match standing {
            Standing::NeedsYou => RunWord::NeedsYou,
            Standing::Stalled => RunWord::Stalled,
            Standing::InFlight => RunWord::Running,
            Standing::Closed => RunWord::Finished,
        };
        RunRow {
            run_id: RunId::from_static(id),
            standing,
            word,
            workflow: "review".into(),
            mode: ModeName::default(),
            age: Duration::from_secs(age_secs),
            reason: None,
            command: None,
        }
    }

    /// The handles of the runs a listing names, in the order it names
    /// them — on a row's own line, and on an unreadable run's
    /// `handle: problem`.
    fn listed(text: &str) -> Vec<&str> {
        text.lines()
            .filter_map(|line| line.strip_prefix(INDENT))
            .filter(|row| !row.starts_with(' '))
            .filter_map(|row| {
                row.split_whitespace()
                    .map(|word| word.trim_end_matches(':'))
                    .find(|word| {
                        word.len() == HANDLE_WIDTH
                            && word
                                .chars()
                                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                    })
            })
            .collect()
    }

    fn render(rows: Vec<RunRow>) -> String {
        render_runs(rows, Vec::new(), &Look::plain())
    }

    /// What each of `ids` is called by on a line a person reads.
    fn handles(ids: &[&'static str]) -> Vec<&'static str> {
        ids.iter()
            .map(|id| &id[id.len() - HANDLE_WIDTH..])
            .collect()
    }

    const OLDEST: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P1";
    const TIED_A: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P2";
    const TIED_B: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P3";
    const NEWEST: &str = "01JBZ5X8K3N7Q2W6E4R9T1Y0P4";

    #[test]
    fn a_group_answers_the_run_that_has_waited_longest_first() {
        let text = render(vec![
            row(NEWEST, Standing::NeedsYou, 30),
            row(OLDEST, Standing::NeedsYou, 86_400),
        ]);
        assert_eq!(listed(&text), handles(&[OLDEST, NEWEST]), "{text}");
    }

    #[test]
    fn runs_that_arrived_together_are_ordered_by_id() {
        let text = render(vec![
            row(TIED_B, Standing::InFlight, 60),
            row(TIED_A, Standing::InFlight, 60),
        ]);
        assert_eq!(listed(&text), handles(&[TIED_A, TIED_B]), "{text}");
    }

    #[test]
    fn what_needs_a_person_is_listed_above_what_is_only_moving() {
        let text = render(vec![
            row(NEWEST, Standing::Closed, 900),
            row(TIED_A, Standing::InFlight, 600),
            row(OLDEST, Standing::NeedsYou, 5),
        ]);
        assert_eq!(listed(&text), handles(&[OLDEST, TIED_A, NEWEST]), "{text}");
        assert!(text.contains("needs you (1)"), "{text}");
        assert!(text.contains("running (1)"), "{text}");
        assert!(text.contains("closed (1)"), "{text}");
    }

    #[test]
    fn a_run_that_does_not_read_back_is_listed_last_and_by_id() {
        let text = render_runs(
            vec![row(OLDEST, Standing::Closed, 10)],
            vec![
                Unreadable::new(&RunId::from_static(NEWEST), "gone".to_string()),
                Unreadable::new(&RunId::from_static(TIED_A), "gone".to_string()),
            ],
            &Look::plain(),
        );
        assert_eq!(listed(&text), handles(&[OLDEST, TIED_A, NEWEST]), "{text}");
        assert!(text.contains("unreadable (2)"), "{text}");
    }

    #[test]
    fn a_run_that_needs_you_gets_its_reason_and_command() {
        let reason = "node `lint` failed and its 0 re-routes to `fix-lint` are exhausted — \
                      exit 1, after it printed what the compiler said about the crate";
        let waiting = RunRow {
            reason: Some(reason.to_string()),
            command: Some(format!("yunta status {}", &OLDEST[OLDEST.len() - 6..])),
            ..row(OLDEST, Standing::NeedsYou, 5)
        };
        let text = render(vec![waiting]);
        let said: Vec<&str> = text.split_whitespace().collect();
        assert!(
            said.join(" ").contains(reason),
            "the reason is said whole, never cut: {text}"
        );
        assert!(
            text.lines()
                .any(|line| line.trim() == format!("yunta status {}", handles(&[OLDEST])[0])),
            "and the command that moves the run has its own line: {text}"
        );
        for line in text.lines() {
            assert!(cell_width(line) <= Look::plain().width.cells(), "{line}");
        }
    }

    #[test]
    fn old_closed_runs_are_folded_into_a_count() {
        let closed: Vec<RunRow> = (0..CLOSED_SHOWN + 2)
            .map(|at| RunRow {
                run_id: RunId::from(format!("01JBZ5X8K3N7Q2W6E4R9T1Y{at:03}").as_str()),
                ..row(OLDEST, Standing::Closed, 60 * (at as u64 + 1))
            })
            .collect();
        let text = render(closed);
        assert_eq!(listed(&text).len(), CLOSED_SHOWN, "{text}");
        assert!(
            listed(&text).first() == Some(&"T1Y000"),
            "the newest closed run comes first: {text}"
        );
        assert!(text.contains("closed (12)"), "{text}");
        assert!(text.contains("2 older closed runs not listed"), "{text}");
    }

    #[test]
    fn an_age_of_days_fits_its_column() {
        let text = render(vec![
            row(OLDEST, Standing::InFlight, 5 * 86_400 + 13 * 3_600),
            row(NEWEST, Standing::InFlight, 30),
        ]);
        let rows: Vec<&str> = text
            .lines()
            .filter(|line| line.contains("review"))
            .collect();
        assert_eq!(rows.len(), 2, "{text}");
        assert!(rows[0].ends_with("5d13h"), "{text}");
        assert_eq!(
            cell_width(rows[0]),
            cell_width(rows[1]),
            "the ages end in one column: {text}"
        );
    }
}
