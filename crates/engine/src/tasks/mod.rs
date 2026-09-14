//! What a tasks document means to the run that holds it: the
//! registration every one entails, and what a document another run
//! hands over carries into the tree this run works in.

mod crossing;

use std::collections::BTreeMap;

use yunta_core::events::{
    self, EventPayload, StoredEvent, TaskLedger, TaskRegisteredPayload, TaskStatus,
    TaskStatusChangedPayload,
};
use yunta_core::ScopeGlob;
use yunta_core::{CommitSha, NodeId, Task, TaskId, TasksFile};

use crate::run::RunError;
use crate::run_log::RunLog;
use yunta_core::events::TaskEvent;

pub(crate) use crossing::{carried_into, standing_of, Standing};

/// Where a tasks document came from, as far as its registration cares.
#[derive(Clone, Copy)]
pub(crate) enum Provenance<'a> {
    /// Nobody did anything about these tasks before this run.
    Fresh,
    /// Another run handed the document over, and this is what of it the
    /// receiving tree already has.
    Inherited {
        carried: &'a BTreeMap<TaskId, CommitSha>,
    },
}

/// What a task is, for the question of whether a status still describes
/// it: same `id` is the same task only with the same criteria and scope.
/// `depends_on` is deliberately outside — an edge says when a task may
/// run, never what passing it means.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Identity {
    criteria: Vec<events::Criterion>,
    scope: Vec<ScopeGlob>,
}

impl Identity {
    fn of(task: &Task) -> Self {
        Identity {
            criteria: task.criteria.iter().map(Into::into).collect(),
            scope: task.scope.clone(),
        }
    }
}

/// The most recent registration this log holds per task id — what a
/// re-registration of the same id is compared against.
pub(crate) fn prior_registrations(events: &[StoredEvent]) -> BTreeMap<TaskId, Identity> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Tasks(TaskEvent::Registered(p))) => Some((
                p.task_id.clone(),
                Identity {
                    criteria: p.criteria.clone(),
                    scope: p.scope.clone(),
                },
            )),
            _ => None,
        })
        .collect()
}

/// What follows a registration, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Follow {
    /// This log registered the id with another identity: whatever it
    /// says about the task is about a different task, so the new one
    /// starts `pending`.
    Reset,
    /// The work is in this run's tree, at the commit the source named —
    /// carried forward so a run that inherits from this one can answer
    /// the same question.
    Done(CommitSha),
}

/// One task of a document, and what its registration entails.
pub(crate) struct Planned<'a> {
    pub task: &'a Task,
    pub follow: Option<Follow>,
}

/// What registering `document` states, one entry per task in document
/// order: a total function of what the two logs say.
///
/// A reset wins over a `done` that crossed, for the reason a reset
/// exists at all — a verdict about a task cut differently says nothing
/// about this one. A task this log already holds `done` gains nothing
/// from a second `done`, and replay ignores a re-registration of a task
/// it already knows, so the plan states no event for it.
pub(crate) fn plan_registration<'a>(
    document: &'a TasksFile,
    prior: &BTreeMap<TaskId, Identity>,
    current: &TaskLedger,
    carried: &BTreeMap<TaskId, CommitSha>,
) -> Vec<Planned<'a>> {
    document
        .tasks
        .iter()
        .map(|task| {
            let recut = prior
                .get(&task.id)
                .is_some_and(|identity| *identity != Identity::of(task));
            let follow = if recut {
                Some(Follow::Reset)
            } else {
                carried
                    .get(&task.id)
                    .filter(|_| current.status(&task.id) != Some(TaskStatus::Done))
                    .map(|commit| Follow::Done(commit.clone()))
            };
            Planned { task, follow }
        })
        .collect()
}

/// Registers every task of `document` on this run's log, in document
/// order, under the node the document entered through — `None` at
/// birth, where no node of this run brought it in.
///
/// Accepting a document says what the run holds; a `task_registered`
/// says what the run has to do about it, and until one exists a task is
/// not a task of this run. What follows each registration is
/// [`plan_registration`]'s call, over this log as it stands right now —
/// so two documents entering at once see each other's registrations.
pub(crate) async fn register(
    log: &RunLog<'_>,
    node: Option<&NodeId>,
    document: &TasksFile,
    provenance: Provenance<'_>,
) -> Result<(), RunError> {
    let events = log.events().await?;
    let prior = prior_registrations(&events);
    let current = crate::replay::derive(&events).tasks;
    let nothing = BTreeMap::new();
    let carried = match provenance {
        Provenance::Fresh => &nothing,
        Provenance::Inherited { carried } => carried,
    };

    for planned in plan_registration(document, &prior, &current, carried) {
        let registered = log
            .record(
                node,
                EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
                    task_id: planned.task.id.clone(),
                    criteria: planned.task.criteria.iter().map(Into::into).collect(),
                    scope: planned.task.scope.clone(),
                    depends_on: planned.task.depends_on.clone(),
                })),
            )
            .await?;
        let Some(follow) = planned.follow else {
            continue;
        };
        let task_id = planned.task.id.clone();
        let changed = match follow {
            Follow::Reset => TaskStatusChangedPayload::to(task_id, TaskStatus::Pending, registered),
            Follow::Done(commit) => TaskStatusChangedPayload::done(task_id, registered, commit),
        };
        log.record(node, EventPayload::Tasks(TaskEvent::StatusChanged(changed)))
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use yunta_core::RunId;
    use yunta_testkit::tasks_document;

    use super::*;

    /// The registration this log holds for `task`, at `seq`.
    fn registration(seq: u64, task: &Task) -> StoredEvent {
        yunta_testkit::stored(
            &RunId::from("run-1"),
            seq,
            yunta_testkit::task_registered(task),
        )
    }

    /// The commit a `done` that crossed names. Any commit: what
    /// `plan_registration` does with it never depends on its value.
    fn landed() -> CommitSha {
        CommitSha::from("0123456789abcdef0123456789abcdef01234567")
    }

    /// What a receiving tree already has of `ids`, all at the same
    /// commit.
    fn carried(ids: &[&str]) -> BTreeMap<TaskId, CommitSha> {
        ids.iter().map(|id| ((*id).into(), landed())).collect()
    }

    /// A ledger holding exactly these statuses, built the way a run
    /// builds one: the registration first, then the move. A fixture can
    /// only state what a log could have written.
    fn ledger_of<S: AsRef<str>>(entries: &[(S, TaskStatus)]) -> TaskLedger {
        let mut ledger = TaskLedger::default();
        for (seq, (id, status)) in entries.iter().enumerate() {
            let task: TaskId = id.as_ref().into();
            let meta = yunta_core::events::EventMeta {
                seq: (seq as u64 + 1).into(),
                at: chrono::DateTime::UNIX_EPOCH,
                node: None,
            };
            ledger
                .apply(
                    &TaskEvent::Registered(TaskRegisteredPayload {
                        task_id: task.clone(),
                        criteria: Vec::new(),
                        scope: Vec::new(),
                        depends_on: Vec::new(),
                    }),
                    &meta,
                )
                .expect("a registration introduces its own task");
            ledger
                .apply(
                    &TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
                        task, *status, meta.seq,
                    )),
                    &meta,
                )
                .expect("the registration above introduced it");
        }
        ledger
    }

    #[test]
    fn a_fresh_document_registers_every_task_with_no_status_to_follow() {
        let doc = tasks_document(&[
            ("T001", "a.txt", "test -f a.txt"),
            ("T002", "b.txt", "true"),
        ]);
        let planned = plan_registration(
            &doc,
            &BTreeMap::new(),
            &TaskLedger::default(),
            &carried(&[]),
        );

        assert_eq!(
            planned
                .iter()
                .map(|p| p.task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["T001", "T002"],
            "one entry per task, in document order"
        );
        assert!(planned.iter().all(|p| p.follow.is_none()));
    }

    #[test]
    fn a_task_that_crossed_with_the_same_identity_is_done_here_at_the_commit_it_names() {
        let doc = tasks_document(&[("T001", "a.txt", "test -f a.txt")]);
        let planned = plan_registration(
            &doc,
            &BTreeMap::new(),
            &TaskLedger::default(),
            &carried(&["T001"]),
        );

        assert_eq!(
            planned[0].follow,
            Some(Follow::Done(landed())),
            "the done carries the commit forward, so a run inheriting from this one can \
             answer the same question"
        );
    }

    #[test]
    fn a_task_that_crossed_whose_identity_changed_here_starts_over() {
        let doc = tasks_document(&[("T001", "a.txt", "test -f a.txt")]);
        let cut_differently = tasks_document(&[("T001", "a.txt", "test -f something-else")]);
        let prior = prior_registrations(&[registration(1, &cut_differently.tasks[0])]);

        let planned = plan_registration(&doc, &prior, &TaskLedger::default(), &carried(&["T001"]));

        assert_eq!(
            planned[0].follow,
            Some(Follow::Reset),
            "a done from elsewhere says nothing about a task this run re-cut"
        );
    }

    #[test]
    fn a_task_this_log_already_has_done_gets_no_second_done() {
        let doc = tasks_document(&[("T001", "a.txt", "test -f a.txt")]);
        let current = ledger_of(&[("T001", TaskStatus::Done)]);

        let planned = plan_registration(&doc, &BTreeMap::new(), &current, &carried(&["T001"]));

        assert_eq!(planned[0].follow, None);
    }

    fn any_status() -> impl Strategy<Value = TaskStatus> {
        prop_oneof![
            Just(TaskStatus::Pending),
            Just(TaskStatus::Ready),
            Just(TaskStatus::Running),
            Just(TaskStatus::Done),
            Just(TaskStatus::Blocked),
            Just(TaskStatus::Failed),
        ]
    }

    /// Ids from a tiny pool, so a generated document, its prior
    /// registrations and what crossed actually talk about the same
    /// tasks.
    fn ids() -> impl Strategy<Value = Vec<(String, String)>> {
        prop::collection::vec(
            (
                prop_oneof![Just("T001"), Just("T002"), Just("T003")].prop_map(str::to_string),
                prop_oneof![Just("true"), Just("false")].prop_map(str::to_string),
            ),
            0..6,
        )
    }

    proptest! {
        /// A registration plan is total and ordered — one entry per task
        /// of the document, in its order — and never carries a `done`
        /// onto a task whose identity this log knows differently.
        #[test]
        fn a_registration_plan_never_marks_done_a_task_whose_identity_changed(
            declared in ids(),
            registered in ids(),
            current in prop::collection::vec(
                (prop_oneof![Just("T001"), Just("T002"), Just("T003")], any_status()),
                0..4,
            ),
            crossed in prop::collection::vec(
                prop_oneof![Just("T001"), Just("T002"), Just("T003")],
                0..4,
            ),
        ) {
            // A document declares each id once, so the generated pairs
            // collapse by id before they become one.
            let unique = |pairs: &[(String, String)]| -> Vec<(String, String)> {
                let mut seen = std::collections::BTreeSet::new();
                pairs.iter().filter(|(id, _)| seen.insert(id.clone())).cloned().collect()
            };
            // A scope of its own per id, because a document whose tasks
            // overlap is not a document at all.
            let as_document = |pairs: &[(String, String)]| {
                let scoped: Vec<(String, String, String)> = unique(pairs)
                    .into_iter()
                    .map(|(id, cmd)| (format!("{id}.txt"), id, cmd))
                    .collect();
                tasks_document(
                    &scoped
                        .iter()
                        .map(|(scope, id, cmd)| (id.as_str(), scope.as_str(), cmd.as_str()))
                        .collect::<Vec<_>>(),
                )
            };
            let doc = as_document(&declared);
            let before = as_document(&registered);
            let prior = prior_registrations(
                &before
                    .tasks
                    .iter()
                    .enumerate()
                    .map(|(i, task)| registration(i as u64 + 1, task))
                    .collect::<Vec<_>>(),
            );
            let current = ledger_of(&current);
            let crossed = carried(&crossed);

            let planned = plan_registration(&doc, &prior, &current, &crossed);

            prop_assert_eq!(
                planned.iter().map(|p| p.task.id.clone()).collect::<Vec<_>>(),
                doc.tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>()
            );
            for entry in &planned {
                if prior
                    .get(&entry.task.id)
                    .is_some_and(|identity| *identity != Identity::of(entry.task))
                {
                    prop_assert_eq!(entry.follow.clone(), Some(Follow::Reset));
                }
            }
        }
    }
}
