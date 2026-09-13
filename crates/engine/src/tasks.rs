//! What a tasks document means to the run that holds it: the
//! registration every one entails, and what a document another run
//! hands over carries with it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use yunta_core::events::{
    self, EventPayload, StoredEvent, TaskRegisteredPayload, TaskStatus, TaskStatusChangedPayload,
};
use yunta_core::{NodeId, RunId, Task, TaskId, TasksFile};

use crate::replay::RunState;
use crate::run::RunError;
use crate::run_log::RunLog;

/// Where a tasks document came from, as far as its registration cares.
#[derive(Clone, Copy)]
pub(crate) enum Provenance<'a> {
    /// Nobody did anything about these tasks before this run.
    Fresh,
    /// Another run's log already says what became of them.
    Inherited { standing: &'a RunState },
}

/// What a task is, for the question of whether a status still describes
/// it: same `id` is the same task only with the same criteria and scope.
/// `depends_on` is deliberately outside — an edge says when a task may
/// run, never what passing it means.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Identity {
    criteria: Vec<events::Criterion>,
    scope: Vec<String>,
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
            Some(EventPayload::TaskRegistered(p)) => Some((
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

/// The state another run's log leaves standing — the one thing this
/// module reads from a log that is not this run's.
///
/// A log that does not replay is refused here rather than read
/// partially: a source that cannot answer for its own tasks is not a
/// source this run can build on, and the diagnostic names it and what
/// stopped the replay.
pub(crate) fn standing_of(run: &RunId, events: &[StoredEvent]) -> Result<RunState, RunError> {
    let state = crate::replay::derive(events);
    match state.broken {
        Some(diagnostic) => Err(RunError::Broken {
            diagnostic: format!(
                "run `{run}` hands over a tasks document, and its own log does not replay: \
                 {diagnostic}"
            ),
        }),
        None => Ok(state),
    }
}

/// The ids of `document` the source leaves `Done`.
///
/// Only `done` crosses: it is the one state the receiving run can stand
/// behind, because that run's tree is built on the tree where the work
/// it names is integrated. An id the source knows and the document no
/// longer declares is not this document's business.
pub(crate) fn carried_done(standing: &RunState, document: &TasksFile) -> BTreeSet<TaskId> {
    document
        .tasks
        .iter()
        .filter(|task| standing.tasks.get(&task.id) == Some(&TaskStatus::Done))
        .map(|task| task.id.clone())
        .collect()
}

/// What follows a registration, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Follow {
    /// This log registered the id with another identity: whatever it
    /// says about the task is about a different task, so the new one
    /// starts `pending`.
    Reset,
    /// The run that handed the document over left the task done, and
    /// this log has nothing that contradicts it.
    Done,
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
    current: &HashMap<TaskId, TaskStatus>,
    carried: &BTreeSet<TaskId>,
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
            } else if carried.contains(&task.id) && current.get(&task.id) != Some(&TaskStatus::Done)
            {
                Some(Follow::Done)
            } else {
                None
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
    let carried = match provenance {
        Provenance::Fresh => BTreeSet::new(),
        Provenance::Inherited { standing } => carried_done(standing, document),
    };

    for planned in plan_registration(document, &prior, &current, &carried) {
        let registered = log
            .record(
                node,
                EventPayload::TaskRegistered(TaskRegisteredPayload {
                    task_id: planned.task.id.clone(),
                    criteria: planned.task.criteria.iter().map(Into::into).collect(),
                    scope: planned.task.scope.clone(),
                    depends_on: planned.task.depends_on.clone(),
                }),
            )
            .await?;
        let Some(follow) = planned.follow else {
            continue;
        };
        log.record(
            node,
            EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                task_id: planned.task.id.clone(),
                new_status: match follow {
                    Follow::Reset => TaskStatus::Pending,
                    Follow::Done => TaskStatus::Done,
                },
                caused_by: registered,
            }),
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use yunta_core::events::{EventBody, TaskStatusChangedPayload};

    use super::*;

    fn document(tasks: &[(&str, &str, &str)]) -> TasksFile {
        let mut yaml = String::from("tasks:\n");
        for (id, scope, cmd) in tasks {
            yaml.push_str(&format!("  - id: {id}\n"));
            yaml.push_str(&format!("    title: \"{id}\"\n"));
            yaml.push_str(&format!("    scope: [\"{scope}\"]\n"));
            yaml.push_str(&format!("    criteria: [{{cmd: \"{cmd}\"}}]\n"));
        }
        yunta_core::shape::read(yaml.as_bytes(), "tasks").expect("a valid tasks document")
    }

    fn standing(tasks: &[(&str, TaskStatus)]) -> RunState {
        RunState {
            tasks: tasks.iter().map(|(id, s)| ((*id).into(), *s)).collect(),
            ..RunState::default()
        }
    }

    fn event(seq: u64, payload: EventPayload) -> StoredEvent {
        StoredEvent {
            run_id: "run-1".into(),
            seq: seq.into(),
            timestamp: chrono::DateTime::UNIX_EPOCH,
            node_id: None,
            body: EventBody::Known(payload),
        }
    }

    fn registration(seq: u64, task: &Task) -> StoredEvent {
        event(
            seq,
            EventPayload::TaskRegistered(TaskRegisteredPayload {
                task_id: task.id.clone(),
                criteria: task.criteria.iter().map(Into::into).collect(),
                scope: task.scope.clone(),
                depends_on: task.depends_on.clone(),
            }),
        )
    }

    #[test]
    fn a_fresh_document_registers_every_task_with_no_status_to_follow() {
        let doc = document(&[
            ("T001", "a.txt", "test -f a.txt"),
            ("T002", "b.txt", "true"),
        ]);
        let planned = plan_registration(&doc, &BTreeMap::new(), &HashMap::new(), &BTreeSet::new());

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
    fn an_inherited_done_task_with_the_same_identity_is_done_here() {
        let doc = document(&[("T001", "a.txt", "test -f a.txt")]);
        let carried = BTreeSet::from([TaskId::from("T001")]);
        let planned = plan_registration(&doc, &BTreeMap::new(), &HashMap::new(), &carried);

        assert_eq!(planned[0].follow, Some(Follow::Done));
    }

    #[test]
    fn an_inherited_done_task_whose_identity_changed_here_starts_over() {
        let doc = document(&[("T001", "a.txt", "test -f a.txt")]);
        let cut_differently = document(&[("T001", "a.txt", "test -f something-else")]);
        let prior = prior_registrations(&[registration(1, &cut_differently.tasks[0])]);
        let carried = BTreeSet::from([TaskId::from("T001")]);

        let planned = plan_registration(&doc, &prior, &HashMap::new(), &carried);

        assert_eq!(
            planned[0].follow,
            Some(Follow::Reset),
            "a done from elsewhere says nothing about a task this run re-cut"
        );
    }

    #[test]
    fn a_task_this_log_already_has_done_gets_no_second_done() {
        let doc = document(&[("T001", "a.txt", "test -f a.txt")]);
        let current = HashMap::from([(TaskId::from("T001"), TaskStatus::Done)]);
        let carried = BTreeSet::from([TaskId::from("T001")]);

        let planned = plan_registration(&doc, &BTreeMap::new(), &current, &carried);

        assert_eq!(planned[0].follow, None);
    }

    #[test]
    fn only_done_crosses_from_a_source_log() {
        let doc = document(&[
            ("T001", "a.txt", "true"),
            ("T002", "b.txt", "true"),
            ("T003", "c.txt", "true"),
            ("T004", "d.txt", "true"),
            ("T005", "e.txt", "true"),
            ("T006", "f.txt", "true"),
        ]);
        let source = standing(&[
            ("T001", TaskStatus::Running),
            ("T002", TaskStatus::Failed),
            ("T003", TaskStatus::Blocked),
            ("T004", TaskStatus::Pending),
            ("T005", TaskStatus::Ready),
            ("T006", TaskStatus::Done),
        ]);

        assert_eq!(
            carried_done(&source, &doc),
            BTreeSet::from([TaskId::from("T006")]),
            "done is the only state with a meaning the receiving run can stand behind"
        );
    }

    #[test]
    fn carried_done_ignores_tasks_the_document_no_longer_has() {
        let doc = document(&[("T001", "a.txt", "true")]);
        let source = standing(&[("T001", TaskStatus::Done), ("T099", TaskStatus::Done)]);

        assert_eq!(
            carried_done(&source, &doc),
            BTreeSet::from([TaskId::from("T001")]),
        );
    }

    #[test]
    fn a_source_log_that_does_not_replay_is_refused_naming_the_run() {
        let orphan = event(
            1,
            EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
                task_id: "T001".into(),
                new_status: TaskStatus::Done,
                caused_by: 1u64.into(),
            }),
        );

        let error = standing_of(&RunId::from("run-source"), &[orphan]).unwrap_err();

        let RunError::Broken { diagnostic } = &error else {
            panic!("a source that cannot answer for itself is refused: {error:?}");
        };
        assert!(
            diagnostic.contains("run-source") && diagnostic.contains("T001"),
            "the diagnostic names the run and what stopped its replay: {diagnostic}"
        );
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
            carried in prop::collection::vec(
                prop_oneof![Just("T001"), Just("T002"), Just("T003")],
                0..4,
            ),
        ) {
            // A document declares each id once, so the generated pairs
            // collapse by id before they become one.
            let unique = |pairs: &[(String, String)]| -> Vec<(String, String)> {
                let mut seen = BTreeSet::new();
                pairs.iter().filter(|(id, _)| seen.insert(id.clone())).cloned().collect()
            };
            // A scope of its own per id, because a document whose tasks
            // overlap is not a document at all.
            let as_document = |pairs: &[(String, String)]| {
                let scoped: Vec<(String, String, String)> = unique(pairs)
                    .into_iter()
                    .map(|(id, cmd)| (format!("{id}.txt"), id, cmd))
                    .collect();
                document(
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
            let current: HashMap<TaskId, TaskStatus> =
                current.into_iter().map(|(id, status)| (id.into(), status)).collect();
            let carried: BTreeSet<TaskId> = carried.into_iter().map(Into::into).collect();

            let planned = plan_registration(&doc, &prior, &current, &carried);

            prop_assert_eq!(
                planned.iter().map(|p| p.task.id.clone()).collect::<Vec<_>>(),
                doc.tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>()
            );
            for entry in &planned {
                if prior
                    .get(&entry.task.id)
                    .is_some_and(|identity| *identity != Identity::of(entry.task))
                {
                    prop_assert_eq!(entry.follow, Some(Follow::Reset));
                }
            }
        }
    }
}
