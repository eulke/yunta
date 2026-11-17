//! A task's scope, one step under its row: each path it declared, then each path a grant
//! added, a row each, marked where a session of the task wrote a path the row's glob
//! selects. A path a person granted says so.
//!
//! What the task's sessions wrote is what the node's ledger keeps of their write and edit
//! calls; on a log whose calls name no session, the task's latest scope check says it
//! instead, as the tree showed it changed.

use std::collections::BTreeSet;
use std::path::PathBuf;

use surco::doc::Block;
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use surco::width::INDENT;
use yunta_core::{ScopeGlob, TaskId};
use yunta_replay::RunState;

/// The words over a task's scope.
const HEADING: &str = "its scope";

/// A task's scope, a path a row: what it declared, then what grants added, each marked where
/// a session of the task wrote a path it selects. Not the node's scope, nor the union of what
/// the run's scope checks found changed.
pub struct TaskScope {
    paths: Vec<ScopePath>,
}

/// One glob of a task's scope: whether a session of the task wrote a path it selects, and
/// who granted it, where a grant brought it in.
struct ScopePath {
    glob: ScopeGlob,
    written: bool,
    granted: Option<GrantedBy>,
}

/// Who granted a path a task did not declare.
enum GrantedBy {
    /// A person, answering the task's request.
    You,
    /// The run's own rule, or the evidence a criterion gave.
    TheRun,
}

impl TaskScope {
    /// `task`'s scope as the run in `state` holds it: the globs its registration froze, then
    /// those its grants added, each marked by what the task's sessions wrote. `None` for a
    /// task the log never registered.
    pub fn of(task: &TaskId, state: &RunState) -> Option<TaskScope> {
        let record = state.tasks.get(task)?;
        let wrote = written_by(task, state);
        let selects = |glob: &ScopeGlob| {
            let matcher = glob.compiled().compile_matcher();
            wrote.iter().any(|path| matcher.is_match(path))
        };
        let derived = state
            .grants
            .derived_for(task)
            .map_or(0, |derived| derived.paths.len());
        let by_you = state.grants.person_granted_to_task(task);
        let mut paths: Vec<ScopePath> = record
            .scope
            .iter()
            .map(|glob| ScopePath {
                glob: glob.clone(),
                written: selects(glob),
                granted: None,
            })
            .collect();
        // What a task reaches beyond its scope holds what its shapes name first, then what
        // grants added, in grant order.
        for glob in state.grants.reach_for(task).into_iter().skip(derived) {
            if paths.iter().any(|path| path.glob == glob) {
                continue;
            }
            let by = match by_you.contains(&glob) {
                true => GrantedBy::You,
                false => GrantedBy::TheRun,
            };
            paths.push(ScopePath {
                written: selects(&glob),
                glob,
                granted: Some(by),
            });
        }
        Some(TaskScope { paths })
    }

    /// Its heading and its paths, one step under the task's row, in one block; nothing for a
    /// task with no scope.
    pub fn blocks(&self, glyphs: Glyphs) -> Vec<Block> {
        if self.paths.is_empty() {
            return Vec::new();
        }
        let heading = Line::new().plain(INDENT).push(Tone::Muted, HEADING);
        let rows = self.paths.iter().map(|path| {
            let mark = match path.written {
                true => Mark::Done,
                false => Mark::Absent,
            };
            let row = Line::new()
                .plain(INDENT)
                .push(Tone::of(mark), glyphs.mark(mark).to_string())
                .plain(format!(" {}", path.glob.as_str()));
            match &path.granted {
                None => row,
                Some(by) => row.push(Tone::Muted, format!(" {} {}", glyphs.sep(), by.words())),
            }
        });
        vec![Block::Rows(std::iter::once(heading).chain(rows).collect())]
    }
}

impl GrantedBy {
    /// What a path's row says of the grant that brought it in.
    fn words(&self) -> &'static str {
        match self {
            GrantedBy::You => "granted by you",
            GrantedBy::TheRun => "granted",
        }
    }
}

/// Every path a session of `task` wrote, as the node ledgers of the run in `state` keep their
/// calls; where a node's calls name no session, the paths the task's latest scope check found
/// changed besides.
fn written_by(task: &TaskId, state: &RunState) -> BTreeSet<PathBuf> {
    let mut wrote = BTreeSet::new();
    let mut unnamed = false;
    for (_, record) in state.nodes.iter() {
        for (session, worked) in &record.session_tasks {
            if worked == task {
                let by = record.written.get(&Some(session.clone()));
                wrote.extend(by.into_iter().flatten().cloned());
            }
        }
        unnamed |= record
            .written
            .get(&None)
            .is_some_and(|paths| !paths.is_empty());
    }
    let checked = state
        .tasks
        .get(task)
        .and_then(|record| record.scope_checked.as_ref());
    if let (true, Some(check)) = (unnamed, checked) {
        wrote.extend(check.diff.iter().cloned());
    }
    wrote
}
