//! A task's proof: its own row, with its mark, its id, its title from the tasks document and
//! its attempt; and one step under it, how it is proved. `proved` says red or green by the
//! task's latest check, the one after its work, or the one before it while none ran after;
//! `by`, each command its criteria run, its guards left out; `keeps`, what the run holds
//! every task to: each command the baseline held, where it was green.

use surco::doc::Block;
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Fill, Line, Tone};
use surco::width::INDENT;
use yunta_core::events::{CriterionType, TaskStatus};
use yunta_core::TaskId;
use yunta_replay::RunState;

/// The cells a proof's labels stand in: the widest, `proved`, and two that part it from its
/// value.
const LABEL: usize = 8;

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
        })
    }

    /// The task's own row, then, one step under it, its proof's rows, a label a column; a
    /// row with nothing to say is left out.
    pub fn blocks(&self, glyphs: Glyphs) -> Vec<Block> {
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
        let task = Block::Rows(vec![self.row(glyphs)]);
        match rows.is_empty() {
            true => vec![task],
            false => vec![task, Block::Rows(rows)],
        }
    }

    /// The task's own row: its mark, its id, its title, and its attempt once it has one.
    fn row(&self, glyphs: Glyphs) -> Line {
        let mark = task_mark(self.status);
        let row = Line::new()
            .push(Tone::of(mark), glyphs.mark(mark).to_string())
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
