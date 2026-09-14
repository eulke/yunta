//! What another run's log leaves standing about the tasks it was given,
//! and what of that the tree a receiving run works in already has.
//!
//! The rule is ancestry, not the door a document came through: a run can
//! stand behind another run's `done` exactly when the work that `done`
//! names is in the tree it is about to work in. A promotion successor
//! branches from its predecessor's tree and so has it; a child sharing
//! its parent's tree has it; a child given a tree of its own leaves its
//! commits on its own branch, and nothing merges them back.

use std::collections::BTreeMap;
use std::path::Path;

use yunta_core::events::{EventPayload, StoredEvent, TaskEvent, TaskStatus};
use yunta_core::{CommitSha, RunId, TaskId, TasksFile};

use crate::process::Supervision;
use crate::run::RunError;

/// What a source run's log leaves standing about the tasks it was given:
/// which it finished, and where each one's work landed.
#[derive(Debug)]
pub(crate) struct Standing {
    /// Every task the log leaves `done`, with the commit that `done`
    /// named. A log written before a `done` carried one leaves `None`,
    /// which is not an answer a receiving run can act on.
    done: BTreeMap<TaskId, Option<CommitSha>>,
}

/// The state another run's log leaves standing — the one thing this
/// module reads from a log that is not this run's.
///
/// A log that does not replay is refused here rather than read
/// partially: a source that cannot answer for its own tasks is not a
/// source this run can build on, and the diagnostic names it and what
/// stopped the replay.
pub(crate) fn standing_of(run: &RunId, events: &[StoredEvent]) -> Result<Standing, RunError> {
    let state = crate::replay::derive(events);
    if let Some(diagnostic) = state.broken {
        return Err(RunError::Broken {
            diagnostic: format!(
                "run `{run}` hands over a tasks document, and its own log does not replay: \
                 {diagnostic}"
            ),
        });
    }
    // Where each `done` put the work, as the log last said it. The
    // status a task ends at is replay's answer; the commit is this
    // log's, and only for the tasks replay leaves `done`.
    let mut placed: BTreeMap<TaskId, Option<CommitSha>> = BTreeMap::new();
    for event in events {
        if let Some(EventPayload::Tasks(TaskEvent::StatusChanged(p))) = event.payload() {
            if p.new_status == TaskStatus::Done {
                placed.insert(p.task_id.clone(), p.commit.clone());
            }
        }
    }
    let done = state
        .tasks
        .iter()
        .filter(|(_, record)| record.status == TaskStatus::Done)
        .map(|(id, _)| (id.clone(), placed.get(id).cloned().flatten()))
        .collect();
    Ok(Standing { done })
}

/// The tasks of `document` whose work `tree` already has: the ones the
/// source left done at a commit `tree`'s HEAD descends from.
///
/// Ancestry decides, not the door the document came through: a receiving
/// run stands behind a `done` exactly when the work it names is in the
/// tree that run is about to work in, and which door a document arrived
/// by only ever approximated that. A `done` whose commit the log does
/// not name answers nothing, so the task starts over — re-verifying
/// costs a session, assuming costs the work. One git call per distinct
/// commit.
pub(crate) async fn carried_into(
    standing: &Standing,
    document: &TasksFile,
    tree: &Path,
    supervision: Supervision<'_>,
) -> Result<BTreeMap<TaskId, CommitSha>, RunError> {
    let placed: Vec<(&TaskId, &CommitSha)> = document
        .tasks
        .iter()
        .filter_map(|task| match standing.done.get(&task.id) {
            Some(Some(commit)) => Some((&task.id, commit)),
            _ => None,
        })
        .collect();
    if placed.is_empty() {
        return Ok(BTreeMap::new());
    }

    let head = crate::worktree::head_commit(tree, supervision).await?;
    let mut answered: BTreeMap<&CommitSha, bool> = BTreeMap::new();
    let mut carried = BTreeMap::new();
    for (id, commit) in placed {
        let in_tree = match answered.get(commit) {
            Some(answer) => *answer,
            None => {
                let answer = has_commit(tree, commit, &head, supervision).await?;
                answered.insert(commit, answer);
                answer
            }
        };
        if in_tree {
            carried.insert(id.clone(), commit.clone());
        }
    }
    Ok(carried)
}

/// Whether `head` descends from `commit`. A non-zero exit is the answer
/// "no", not a failure: git says the same when the commit is on a branch
/// this tree never took and when this repository does not have it at
/// all, and either way the work is not here.
async fn has_commit(
    tree: &Path,
    commit: &CommitSha,
    head: &CommitSha,
    supervision: Supervision<'_>,
) -> Result<bool, RunError> {
    crate::git::success(
        tree,
        &[
            "merge-base",
            "--is-ancestor",
            commit.as_str(),
            head.as_str(),
        ],
        supervision,
    )
    .await
    .map_err(RunError::Git)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use yunta_core::{CommitSha, RunId, Task, TaskId};
    use yunta_testkit::{git, git_output, init_repo, tasks_document, INITIAL_BRANCH};

    use super::*;
    use crate::run::RunError;

    /// A fresh repository, as the tree a run receiving a document is
    /// about to work in.
    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temp dir");
        init_repo(dir.path());
        dir
    }

    /// Commits `name` on `tree`'s current branch and answers with the
    /// commit that made — where a task's work lands.
    fn commit(tree: &Path, name: &str) -> CommitSha {
        std::fs::write(tree.join(name), name).expect("write the file");
        git(tree, &["add", "."]);
        git(tree, &["commit", "-q", "-m", name]);
        git_output(tree, &["rev-parse", "HEAD"])
            .parse()
            .expect("a commit sha")
    }

    /// The source run every test here reads from.
    fn source() -> RunId {
        RunId::from("run-source")
    }

    /// A source run's log: each task registered, then left where the
    /// entry says, at the commit the entry names.
    fn source_log(entries: &[(&Task, TaskStatus, Option<&CommitSha>)]) -> Vec<StoredEvent> {
        let run = source();
        let mut events = Vec::new();
        for (task, status, commit) in entries {
            let registered = events.len() as u64 + 1;
            events.push(yunta_testkit::stored(
                &run,
                registered,
                yunta_testkit::task_registered(task),
            ));
            // Every status carries a commit here, including the five
            // that have no business naming one: the point is what a
            // receiving run stands behind when it reads a log that
            // holds them anyway.
            let changed = match commit {
                Some(commit) => yunta_testkit::status_changed_carrying(
                    &task.id,
                    *status,
                    commit,
                    registered.into(),
                ),
                None => {
                    yunta_testkit::task_status_changed(&task.id, *status, None, registered.into())
                }
            };
            events.push(yunta_testkit::stored(&run, registered + 1, changed));
        }
        events
    }

    #[tokio::test]
    async fn work_in_the_tree_crosses() {
        let tree = tree();
        let landed = commit(tree.path(), "a.txt");
        let document = tasks_document(&[("T001", "a.txt", "test -f a.txt")]);
        let standing = standing_of(
            &source(),
            &source_log(&[(&document.tasks[0], TaskStatus::Done, Some(&landed))]),
        )
        .expect("a log that replays");

        let carried = carried_into(&standing, &document, tree.path(), Supervision::none())
            .await
            .expect("git answers");

        assert_eq!(
            carried,
            BTreeMap::from([(TaskId::from("T001"), landed)]),
            "the tree's HEAD descends from where the work landed, so the task crosses"
        );
    }

    #[tokio::test]
    async fn work_on_a_branch_the_tree_never_took_does_not_cross() {
        let tree = tree();
        git(tree.path(), &["checkout", "-q", "-b", "aside"]);
        let aside = commit(tree.path(), "a.txt");
        git(tree.path(), &["checkout", "-q", INITIAL_BRANCH]);
        let document = tasks_document(&[("T001", "a.txt", "test -f a.txt")]);
        let standing = standing_of(
            &source(),
            &source_log(&[(&document.tasks[0], TaskStatus::Done, Some(&aside))]),
        )
        .expect("a log that replays");

        let carried = carried_into(&standing, &document, tree.path(), Supervision::none())
            .await
            .expect("git answers");

        assert!(
            carried.is_empty(),
            "the work sits on a branch this tree never took: {carried:?}"
        );
    }

    #[tokio::test]
    async fn a_done_the_log_never_placed_does_not_cross() {
        let tree = tree();
        let document = tasks_document(&[("T001", "a.txt", "test -f a.txt")]);
        let standing = standing_of(
            &source(),
            &source_log(&[(&document.tasks[0], TaskStatus::Done, None)]),
        )
        .expect("a log that replays");

        let carried = carried_into(&standing, &document, tree.path(), Supervision::none())
            .await
            .expect("git answers");

        assert!(
            carried.is_empty(),
            "a done that names no commit answers nothing about any tree: {carried:?}"
        );
    }

    #[tokio::test]
    async fn a_task_the_document_no_longer_has_is_not_carried() {
        let tree = tree();
        let landed = commit(tree.path(), "a.txt");
        let source_document = tasks_document(&[
            ("T001", "a.txt", "test -f a.txt"),
            ("T099", "z.txt", "test -f z.txt"),
        ]);
        let standing = standing_of(
            &source(),
            &source_log(&[
                (&source_document.tasks[0], TaskStatus::Done, Some(&landed)),
                (&source_document.tasks[1], TaskStatus::Done, Some(&landed)),
            ]),
        )
        .expect("a log that replays");
        let document = tasks_document(&[("T001", "a.txt", "test -f a.txt")]);

        let carried = carried_into(&standing, &document, tree.path(), Supervision::none())
            .await
            .expect("git answers");

        assert_eq!(
            carried,
            BTreeMap::from([(TaskId::from("T001"), landed)]),
            "an id the source knows and the document no longer declares is not this \
             document's business"
        );
    }

    #[tokio::test]
    async fn only_done_stands_from_a_source_log() {
        let tree = tree();
        let landed = commit(tree.path(), "a.txt");
        let document = tasks_document(&[
            ("T001", "a.txt", "true"),
            ("T002", "b.txt", "true"),
            ("T003", "c.txt", "true"),
            ("T004", "d.txt", "true"),
            ("T005", "e.txt", "true"),
            ("T006", "f.txt", "true"),
        ]);
        let every_status = [
            TaskStatus::Running,
            TaskStatus::Failed,
            TaskStatus::Blocked,
            TaskStatus::Pending,
            TaskStatus::Ready,
            TaskStatus::Done,
        ];
        let entries: Vec<(&Task, TaskStatus, Option<&CommitSha>)> = document
            .tasks
            .iter()
            .zip(every_status)
            .map(|(task, status)| (task, status, Some(&landed)))
            .collect();
        let standing = standing_of(&source(), &source_log(&entries)).expect("a log that replays");

        let carried = carried_into(&standing, &document, tree.path(), Supervision::none())
            .await
            .expect("git answers");

        assert_eq!(
            carried,
            BTreeMap::from([(TaskId::from("T006"), landed)]),
            "done is the only state with a meaning the receiving run can stand behind"
        );
    }

    #[test]
    fn a_source_log_that_does_not_replay_is_refused_naming_the_run() {
        // A status about a task nobody registered: a log stops replaying
        // right there.
        let orphan = yunta_testkit::stored(
            &source(),
            1,
            yunta_testkit::task_status_changed(
                &TaskId::from("T001"),
                TaskStatus::Done,
                None,
                1u64.into(),
            ),
        );

        let error = standing_of(&source(), &[orphan]).unwrap_err();

        let RunError::Broken { diagnostic } = &error else {
            panic!("a source that cannot answer for itself is refused: {error:?}");
        };
        assert!(
            diagnostic.contains("run-source") && diagnostic.contains("T001"),
            "the diagnostic names the run and what stopped its replay: {diagnostic}"
        );
    }
}
