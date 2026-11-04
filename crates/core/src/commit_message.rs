//! What a commit a run makes says about where it came from: git trailers
//! under its message, naming the run, the node and the task that made
//! it, so `git log --grep` and `git interpret-trailers` find a run's
//! commits on any clone, without the run's log.
//!
//! Every subject a run writes is worded here, so a person reading `git
//! log` meets the same words on every commit a run makes.

use std::path::{Path, PathBuf};

use surco::text::{counted, one_line};

use crate::{NodeId, RunId, TaskId};

/// The trailer that names the run a commit came from.
pub const RUN_TRAILER: &str = "Yunta-Run";
/// The trailer that names the node whose work a commit holds.
pub const NODE_TRAILER: &str = "Yunta-Node";
/// The trailer that names the task whose work a commit holds.
pub const TASK_TRAILER: &str = "Yunta-Task";

/// The most characters a subject takes: what `git log --oneline` and a forge's list of commits
/// show on one row without cutting it themselves.
pub const SUBJECT_MOST: usize = 72;

/// What stands where a subject stops short of what it says: plain text, since a subject is
/// read by tools that may not read anything else.
const STOPS_SHORT: &str = "...";

/// A commit message a run writes: its subject, what is said under it, and a trailer for the
/// run and, when one made it, for the node and the task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMessage {
    subject: String,
    below: Vec<String>,
    run: RunId,
    node: Option<NodeId>,
    task: Option<TaskId>,
}

impl CommitMessage {
    /// What a task's commit says: its title, under the task and the node it runs in.
    pub fn of_task(run: &RunId, node: &NodeId, task: &TaskId, title: &str) -> Self {
        Self::saying(format!("task {task}: {title}"), run)
            .node(node)
            .task(task)
    }

    /// What a task's commit says when its last attempt left work and stopped there, so
    /// the work is held under the task rather than under any attempt's outcome.
    pub fn of_left_task(run: &RunId, node: &NodeId, task: &TaskId) -> Self {
        Self::saying(format!("task {task}: what its last attempt left"), run)
            .node(node)
            .task(task)
    }

    /// What a node's commit says: what the node declares it does, or else what it changed, read
    /// from the index after staging, or else just its id. `beside` names the nodes whose work the
    /// commit holds too.
    pub fn of_node(
        run: &RunId,
        node: &NodeId,
        description: Option<&str>,
        changed: &[PathBuf],
        beside: &[String],
    ) -> Self {
        let head = format!("node {node}");
        let described = description.map(one_line).filter(|said| !said.is_empty());
        let subject = match described {
            Some(said) => format!("{head}: {said}"),
            None => match changes(changed, &format!("{head}: ")) {
                Some(words) => format!("{head}: {words}"),
                None => head,
            },
        };
        Self::saying(subject, run).node(node).holding_beside(beside)
    }

    /// What a node's commit says when its attempt failed: which attempt left the work.
    pub fn of_failed_node(run: &RunId, node: &NodeId, attempt: u32, beside: &[String]) -> Self {
        Self::saying(
            format!("node {node}: what attempt {attempt} left when it failed"),
            run,
        )
        .node(node)
        .holding_beside(beside)
    }

    /// What a commit of the tree a node found in the run's tree says: that the work was there
    /// before the node started, and, when an earlier attempt of the node was interrupted, which
    /// attempt left it.
    pub fn of_found(run: &RunId, node: &NodeId, interrupted: Option<u32>) -> Self {
        let message = Self::saying(
            format!("found in the run's tree before node {node} started"),
            run,
        )
        .node(node);
        match interrupted {
            Some(attempt) => message.below_line(format!(
                "Holds what attempt {attempt} left when it was interrupted."
            )),
            None => message,
        }
    }

    /// What the first commit of a unit's own tree says: the node it started for, so a person
    /// reading the branch knows whose tree it is.
    pub fn of_unit_start(run: &RunId, node: &NodeId) -> Self {
        Self::saying(format!("the tree node {node} started from"), run).node(node)
    }

    /// What the distilled knowledge's commit says: the run it was distilled from, by its handle,
    /// since a person reads a run by its handle and the trailer holds the whole id.
    pub fn of_distilled(run: &RunId) -> Self {
        Self::saying(
            format!("docs(knowledge): distill from {}", run.handle()),
            run,
        )
    }

    /// What a gate's published file says: the path and the gate that published it.
    pub fn of_published(run: &RunId, node: &NodeId, path: &str) -> Self {
        Self::saying(format!("publish {path} for gate {node}"), run).node(node)
    }

    /// The same message, naming `task` as the work it holds.
    pub fn task(mut self, task: &TaskId) -> Self {
        self.task = Some(task.clone());
        self
    }

    /// The same message, naming `node` as what made the commit.
    fn node(mut self, node: &NodeId) -> Self {
        self.node = Some(node.clone());
        self
    }

    /// The same message, with `paragraph` said under its subject.
    fn below_line(mut self, paragraph: String) -> Self {
        self.below.push(paragraph);
        self
    }

    /// The same message, saying under its subject which nodes' work it holds beside its own, when
    /// there are any.
    fn holding_beside(self, beside: &[String]) -> Self {
        match beside.is_empty() {
            true => self,
            false => self.below_line(format!(
                "Holds what {} left in the run's tree while this node worked.",
                beside.join(", ")
            )),
        }
    }

    /// A message whose subject says `subject`, folded to one line. A subject longer than
    /// `SUBJECT_MOST` characters is stopped at a word, and what it says whole goes below it.
    fn saying(subject: String, run: &RunId) -> Self {
        let line = one_line(&subject);
        let (subject, below) = match line.chars().count() > SUBJECT_MOST {
            true => (stopped(&line), vec![line]),
            false => (line, Vec::new()),
        };
        CommitMessage {
            subject,
            below,
            run: run.clone(),
            node: None,
            task: None,
        }
    }

    /// The text git records: the subject, what is said below it, and the trailers, a blank
    /// line between each.
    pub fn text(&self) -> String {
        let mut paragraphs = vec![self.subject.clone()];
        paragraphs.extend(self.below.iter().cloned());
        paragraphs.push(self.trailers());
        paragraphs.join("\n\n")
    }

    /// The trailers, one line each: the run, then the node and the task when they made the commit.
    fn trailers(&self) -> String {
        let mut lines = vec![format!("{RUN_TRAILER}: {}", self.run)];
        if let Some(node) = &self.node {
            lines.push(format!("{NODE_TRAILER}: {node}"));
        }
        if let Some(task) = &self.task {
            lines.push(format!("{TASK_TRAILER}: {task}"));
        }
        lines.join("\n")
    }
}

/// `line`, longer than `SUBJECT_MOST` characters, in at most that many: stopped at the last
/// whole word that leaves room for `STOPS_SHORT`. A line whose last such word stands in its
/// first half has little to gain from it, so it is cut where the room ends instead.
fn stopped(line: &str) -> String {
    let room = SUBJECT_MOST - STOPS_SHORT.chars().count();
    let head: Vec<char> = line.chars().take(room + 1).collect();
    let word_end = (1..=room).rev().find(|&at| {
        head.get(at)
            .is_some_and(|character| character.is_whitespace())
    });
    let cut = match word_end {
        Some(at) if 2 * at > room => at,
        _ => room,
    };
    let kept: String = line.chars().take(cut).collect();
    format!("{kept}{STOPS_SHORT}")
}

/// What `changed` holds, in words that fit a subject opening with `opening`: the path, two
/// paths, or how many files and the directory they share when naming them would not fit.
/// `None` for a commit that holds nothing.
fn changes(changed: &[PathBuf], opening: &str) -> Option<String> {
    let names: Vec<String> = changed
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let named = match names.as_slice() {
        [] => return None,
        [one] => Some(format!("changes {one}")),
        [first, second] => Some(format!("changes {first} and {second}")),
        [..] => None,
    };
    let fits = |words: &str| opening.chars().count() + words.chars().count() <= SUBJECT_MOST;
    if let Some(words) = named.filter(|words| fits(words.as_str())) {
        return Some(words);
    }
    let count = counted(changed.len(), "file");
    let under = shared_directory(changed).map(|dir| format!("changes {count} under {dir}"));
    under
        .filter(|words| fits(words.as_str()))
        .or_else(|| Some(format!("changes {count}")))
}

/// The directory every path in `changed` lies under, written with a trailing `/`, or `None`
/// when they share none. Only directories count: a file's own name is never shared.
fn shared_directory(changed: &[PathBuf]) -> Option<String> {
    let mut dirs = changed.iter().map(|path| directories(path));
    let first = dirs.next()?;
    let shared: Vec<String> = dirs.fold(first, |shared, dir| {
        shared
            .into_iter()
            .zip(dir)
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a)
            .collect()
    });
    (!shared.is_empty()).then(|| format!("{}/", shared.join("/")))
}

/// The directories a path lies in, outermost first, as they are spelled in the path.
fn directories(path: &Path) -> Vec<String> {
    path.parent()
        .map(|dir| {
            dir.components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
