//! `yunta list --runs`: every local run as an inbox — grouped by what a
//! reader can do about it (what stopped on a person, what is still
//! moving, what has closed) and ordered inside each group by how long it
//! has been there, so the run to answer first is the first one read.
//!
//! Which runs it lists, and in which group, is the [`Inbox`]'s answer;
//! this module lays it out.

use yunta_core::units::DURATION_WIDEST;
use yunta_core::RunId;

use super::inbox::{Inbox, RunRow, Standing, Unreadable};
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::render::{cell_width, duration, indent, truncate, Look, INDENT};

/// The cells a run's handle gets: what a reader copies into the next
/// command, so the column pads a shorter one and never cuts one.
const HANDLE_WIDTH: usize = RunId::HANDLE_CHARS;

/// The cells the workflow name and the run's mode share — enough for the
/// names a repo's own workflows carry, and the column the eye runs down
/// to find the run it came for.
const NAME_WIDTH: usize = 44;

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
        print!("{}", render_runs(rows, unreadable, Look::stdout()));
    }
    if elsewhere > 0 {
        println!(
            "\n{} in other projects — yunta list --runs --all",
            yunta_core::text::counted(elsewhere, "run")
        );
    }
    Ok(Outcome::Success)
}

/// The listing itself: what needs a person first, then what is still
/// moving, then what has closed, and last the runs that do not read
/// back. Within a group the run that has been there longest comes first
/// — the one that has been waiting since yesterday is the one to answer
/// before the one that stopped a minute ago — and runs that arrived at
/// their state together are ordered by id, which for a ULID is the order
/// they were created in.
///
/// A run under `unreadable` has no state to have been in for any length
/// of time, so that group is ordered by id alone.
fn render_runs(mut rows: Vec<RunRow>, mut unreadable: Vec<Unreadable>, look: Look) -> String {
    rows.sort_by(|a, b| b.age.cmp(&a.age).then_with(|| a.run_id.cmp(&b.run_id)));
    let mut out = String::new();
    for standing in Standing::ALL {
        let group: Vec<&RunRow> = rows.iter().filter(|row| row.standing == standing).collect();
        if group.is_empty() {
            continue;
        }
        push_heading(&mut out, standing.heading(), group.len());
        for row in group {
            out.push_str(&row.render(look));
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
    /// Two lines: what the run is, then where it stands. The identity
    /// line carries the id a reader copies, the workflow that names what
    /// the run is doing and the mode it does it in; the line under it is
    /// the same summary `yunta status` prints, so the two surfaces say
    /// the same thing about the same run.
    fn render(&self, Look { glyphs, width }: Look) -> String {
        // The row hangs one step under the heading of its group, and
        // its summary one step further under the row, so the summary
        // reads as this run's line rather than the next run's.
        let margin = indent(2);
        format!(
            // The age right-aligned, so a column of them compares as
            // numbers.
            "{INDENT}{:<HANDLE_WIDTH$}  {}  {:>DURATION_WIDEST$}\n{margin}{}\n",
            self.run_id.handle(),
            truncate(
                &format!("{} ({})", self.workflow, self.mode),
                NAME_WIDTH,
                glyphs
            ),
            duration(self.age),
            truncate(
                &self.summary,
                width.cells().saturating_sub(cell_width(&margin)),
                glyphs
            )
            .trim_end(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use yunta_core::{ModeName, RunId};

    use super::*;

    fn row(id: &'static str, standing: Standing, age_secs: u64) -> RunRow {
        RunRow {
            run_id: RunId::from_static(id),
            standing,
            workflow: "review".into(),
            mode: ModeName::default(),
            age: Duration::from_secs(age_secs),
            summary: "1/2 nodes · 0 reroutes · running".to_string(),
        }
    }

    /// The handles of the runs a listing names, in the order it names
    /// them — the first word of every line that opens with one, whether
    /// the line is a row's identity line or an unreadable run's
    /// `handle: problem`.
    fn listed(text: &str) -> Vec<&str> {
        text.lines()
            .filter_map(|line| line.strip_prefix(INDENT))
            .filter(|row| !row.starts_with(' '))
            .filter_map(|row| row.split_whitespace().next())
            .map(|word| word.trim_end_matches(':'))
            .collect()
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
        let text = render_runs(
            vec![
                row(NEWEST, Standing::NeedsYou, 30),
                row(OLDEST, Standing::NeedsYou, 86_400),
            ],
            Vec::new(),
            Look::plain(),
        );
        assert_eq!(listed(&text), handles(&[OLDEST, NEWEST]), "{text}");
    }

    #[test]
    fn runs_that_arrived_together_are_ordered_by_id() {
        let text = render_runs(
            vec![
                row(TIED_B, Standing::InFlight, 60),
                row(TIED_A, Standing::InFlight, 60),
            ],
            Vec::new(),
            Look::plain(),
        );
        assert_eq!(listed(&text), handles(&[TIED_A, TIED_B]), "{text}");
    }

    #[test]
    fn what_needs_a_person_is_listed_above_what_is_only_moving() {
        let text = render_runs(
            vec![
                row(NEWEST, Standing::Closed, 900),
                row(TIED_A, Standing::InFlight, 600),
                row(OLDEST, Standing::NeedsYou, 5),
            ],
            Vec::new(),
            Look::plain(),
        );
        assert_eq!(listed(&text), handles(&[OLDEST, TIED_A, NEWEST]), "{text}");
        assert!(text.contains("needs you (1)"), "{text}");
        assert!(text.contains("in flight (1)"), "{text}");
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
            Look::plain(),
        );
        assert_eq!(listed(&text), handles(&[OLDEST, TIED_A, NEWEST]), "{text}");
        assert!(text.contains("unreadable (2)"), "{text}");
    }
}
