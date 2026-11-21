//! A task's proof: its own row, with its mark, its id, its title from the tasks document and
//! its attempt; and one step under it, how it is proved. `proved` says red or green by the
//! task's latest check, the one after its work, or the one before it while none ran after;
//! `by`, each command its criteria run, its guards left out; `keeps`, what the run holds
//! every task to: each command the baseline held, where it was green.
//!
//! A pane shows a task at work turning while its run moves: its row opens on the spinner's
//! frame of the run's last event in place of its mark, as the node it works under does in
//! the nodes beside the pane; a printed page, which does not move, keeps the mark. A pane
//! short of rows takes the row and its proof apart ([`TaskProof::row`],
//! [`TaskProof::proof`]): which of them stands is its own to say.

use surco::doc::Block;
use surco::glyphs::Mark;
use surco::line::{Fill, Line, Tone};
use surco::look::Look;
use surco::motion::Motion;
use surco::width::INDENT;
use yunta_core::events::{CriterionType, TaskStatus};
use yunta_core::TaskId;
use yunta_replay::RunState;

/// The cells a proof's labels stand in: the widest, `proved`, and two that part it from its
/// value. A pane's other blocks whose labels are a word each stand them in the same column.
pub(super) const LABEL: usize = 8;

/// How a task is proved, by its latest check. Not a criterion's own answer, which a check
/// holds one of for each command it ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProvedAs {
    /// A command of the check answered red.
    Red,
    /// Every command of the check answered green.
    Green,
}

/// A task's own row, and how it is proved: its latest check's verdict, the commands its
/// criteria run, and what the run holds every task to. Not its scope, [`super::TaskScope`].
pub struct TaskProof {
    task: TaskId,
    title: Option<String>,
    status: TaskStatus,
    attempt: u32,
    proved: Option<ProvedAs>,
    by: Vec<String>,
    keeps: Vec<String>,
    /// The events the spinner's frame is turned by while the task works and its run moves:
    /// the seq of the run's last event. None where the row keeps the task's mark.
    turned: Option<u64>,
}

impl TaskProof {
    /// `task`'s proof as the tasks' ledger of the run in `state` holds it, titled `title` as
    /// the tasks document gives it, and held to what the run's baseline held green. `None`
    /// for a task the log never registered.
    pub fn of(task: &TaskId, title: Option<&str>, state: &RunState) -> Option<TaskProof> {
        let record = state.tasks.get(task)?;
        let check = record.checked.as_ref().or(record.checked_before.as_ref());
        let green = check.map(|check| check.results.iter().all(|result| result.exit_code == 0));
        let proved = green.map(|green| match green {
            true => ProvedAs::Green,
            false => ProvedAs::Red,
        });
        let by = record
            .criteria
            .iter()
            .filter(|criterion| !matches!(criterion.r#type, Some(CriterionType::Guard)))
            .map(|criterion| criterion.cmd.clone())
            .collect();
        let keeps = state
            .run
            .held()
            .values()
            .filter(|held| held.passed())
            .map(|held| held.name.to_string())
            .collect();
        Some(TaskProof {
            task: task.clone(),
            title: title.map(str::to_string),
            status: record.status,
            attempt: record.attempts,
            proved,
            by,
            keeps,
            turned: None,
        })
    }

    /// This task as a pane shows it while its run moves: where it works and the look moves,
    /// its row opens on the spinner's frame `turned` events bring it to, the seq of the
    /// run's last event, in place of its mark.
    pub fn turning(mut self, turned: u64) -> TaskProof {
        self.turned = Some(turned);
        self
    }

    /// The task's own row, then, one step under it, its proof's rows, a label a column; a
    /// row with nothing to say is left out.
    pub fn blocks(&self, look: &Look) -> Vec<Block> {
        let task = Block::Rows(vec![self.row(look)]);
        let rows = self.proof();
        match rows.is_empty() {
            true => vec![task],
            false => vec![task, Block::Rows(rows)],
        }
    }

    /// How the task is proved, one step under its row, a label a column: red or green, by
    /// which commands, and what the run holds it to; a row with nothing to say left out.
    pub fn proof(&self) -> Vec<Line> {
        let mut rows = Vec::new();
        if let Some(proved) = self.proved {
            let (tone, word) = match proved {
                ProvedAs::Red => (Tone::Failed, "red"),
                ProvedAs::Green => (Tone::Done, "green"),
            };
            rows.push(labelled("proved").push_filled(tone, Fill::Pill, format!(" {word}")));
        }
        for (at, command) in self.by.iter().enumerate() {
            let row = match at {
                0 => labelled("by"),
                _ => Line::new().plain(INDENT).plain(" ".repeat(LABEL)),
            };
            rows.push(row.push(Tone::Command, command.as_str()));
        }
        if !self.keeps.is_empty() {
            rows.push(
                labelled("keeps")
                    .plain(format!("{}, green at the baseline", self.keeps.join(", "))),
            );
        }
        rows
    }

    /// The task's own row: its mark, or the spinner's frame where it turns, its id, its
    /// title, and its attempt once it has one.
    pub fn row(&self, look: &Look) -> Line {
        let glyphs = look.glyphs;
        let mark = task_mark(self.status);
        let sign = match (self.status, self.turned, look.motion) {
            (TaskStatus::Running, Some(turned), Motion::Moves) => {
                surco::motion::frame(glyphs, turned).to_string()
            }
            _ => glyphs.mark(mark).to_string(),
        };
        let row = Line::new()
            .push(Tone::of(mark), sign)
            .plain(" ")
            .push(Tone::Strong, self.task.as_str());
        let row = match &self.title {
            Some(title) => row.plain(format!("  {title}")),
            None => row,
        };
        match self.attempt {
            0 => row,
            attempt => row.push(Tone::Muted, format!(" {} attempt {attempt}", glyphs.sep())),
        }
    }
}

/// A proof's row, one step under its task's, opening with `label` in its column.
fn labelled(label: &str) -> Line {
    Line::new()
        .plain(INDENT)
        .push(Tone::Muted, format!("{label:<LABEL$}"))
}

/// The mark a task's status carries: what is done, what failed, what a cause it cannot pass
/// on its own stopped, what is under way, and what has not begun.
pub(crate) fn task_mark(status: TaskStatus) -> Mark {
    match status {
        TaskStatus::Done => Mark::Done,
        TaskStatus::Failed => Mark::Failed,
        TaskStatus::Blocked => Mark::Caution,
        TaskStatus::Running => Mark::Running,
        TaskStatus::Pending | TaskStatus::Ready => Mark::Absent,
    }
}
