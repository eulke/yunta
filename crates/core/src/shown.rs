//! A document an escalation shows, read from the run for the person
//! deciding: what the log names, where its view sits, and what it says.

use crate::events::artifacts::HandedOver;
use crate::events::findings::RunFindings;
use crate::events::{AcceptedDeparture, Shown};
use crate::{FindingsFile, SpecFile, TaskId, TasksFile};

/// One document an escalation shows, as the run holds it: what the log
/// names, where its view sits, and what it says.
#[derive(Debug, Clone, PartialEq)]
pub struct ShownDocument {
    pub shown: Shown,
    pub path: std::path::PathBuf,
    pub content: ShownContent,
}

/// A shown document's content: a tasks document read into its tasks, a
/// spec into its specs, a findings document into its findings, any other
/// as its text.
#[derive(Debug, Clone, PartialEq)]
pub enum ShownContent {
    /// A plan, as the run will judge it.
    Tasks(Box<PlanReview>),
    /// The tests a plan's tasks are held to, read into its specs.
    Spec(SpecFile),
    /// What a review found, read into its findings.
    Findings(FindingsFile),
    /// Every finding standing in the run, each with the node that
    /// reported it and how other nodes answered it.
    RunFindings(RunFindings),
    Text(String),
}

/// A plan as the run will judge it: what the planner wrote, and beside
/// it what holds each task — its own criteria, the tests the run's spec
/// gives it, the suite every task keeps passing — and where the plan
/// says a session will change something no session may.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanReview {
    pub plan: TasksFile,
    /// Every departure from the plan a person accepted while its tasks
    /// were built — where the work stops being what the plan says.
    pub departed: Vec<AcceptedDeparture>,
    /// The spec the plan is shown with, when a decision shows both: its
    /// tests are read on their tasks rather than as a second document.
    pub spec: Option<SpecFile>,
    /// The suite the run measured green before any work, which holds
    /// every task.
    pub suite: Option<String>,
    /// Each task's review, in the plan's order.
    pub tasks: Vec<TaskReview>,
    /// How its planner handed it over, when the run's log says.
    pub handed_over: Option<HandedOver>,
}

impl PlanReview {
    /// Everything that keeps this plan from being proven as it is
    /// written, task by task in the plan's order.
    pub fn flaws(&self) -> Vec<Flaw> {
        let mut flaws = Vec::new();
        for (task, review) in self.plan.tasks.iter().zip(&self.tasks) {
            flaws.extend(
                crate::passes_by_a_name(task)
                    .into_iter()
                    .map(|(cmd, file)| Flaw::PassesByAName {
                        task: task.id.clone(),
                        cmd,
                        file,
                    }),
            );
            if let Some(spec) = self.spec.as_ref().and_then(|spec| spec.of(&task.id)) {
                flaws.extend(spec.hollow_tests().map(|test| Flaw::HollowSpecTest {
                    task: task.id.clone(),
                    cmd: test.cmd.clone(),
                }));
                flaws.extend(spec.unrun_files().map(|file| Flaw::UnrunSpecFile {
                    task: task.id.clone(),
                    path: file.path.clone(),
                }));
            }
            flaws.extend(review.denied.iter().map(|change| Flaw::ChangesASpecTest {
                task: task.id.clone(),
                at: change.at.clone(),
                owner: change.owner.clone(),
            }));
            flaws.extend(
                review
                    .criteria
                    .iter()
                    .filter_map(|criterion| match &criterion.from {
                        HeldTo::AnotherTask { task: owner } => {
                            Some(Flaw::JudgedByAnotherTasksTest {
                                task: task.id.clone(),
                                cmd: criterion.cmd.clone(),
                                owner: owner.clone(),
                            })
                        }
                        HeldTo::Plan | HeldTo::Spec => None,
                    }),
            );
        }
        flaws
    }
}

/// What keeps a plan from being proven as it is written: a test that
/// cannot fail for the right reason, or a plan that contradicts the spec
/// that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flaw {
    /// A criterion that passes once `file`, which the task changes, holds
    /// a name — whatever the code does.
    PassesByAName {
        task: TaskId,
        cmd: String,
        file: String,
    },
    /// A test of the task's spec that runs none of the files the spec
    /// wrote.
    HollowSpecTest { task: TaskId, cmd: String },
    /// A file the spec wrote for the task that none of its tests runs.
    UnrunSpecFile { task: TaskId, path: String },
    /// A change the plan names on a test the spec wrote, which no
    /// session may write.
    ChangesASpecTest {
        task: TaskId,
        at: String,
        owner: TaskId,
    },
    /// A criterion that runs the test the spec gives another task: it
    /// passes once that task is done.
    JudgedByAnotherTasksTest {
        task: TaskId,
        cmd: String,
        owner: TaskId,
    },
}

impl Flaw {
    /// The task the flaw is in.
    pub fn task(&self) -> &TaskId {
        match self {
            Flaw::PassesByAName { task, .. }
            | Flaw::HollowSpecTest { task, .. }
            | Flaw::UnrunSpecFile { task, .. }
            | Flaw::ChangesASpecTest { task, .. }
            | Flaw::JudgedByAnotherTasksTest { task, .. } => task,
        }
    }

    /// What the flaw means for the decision.
    pub fn so(&self) -> String {
        match self {
            Flaw::PassesByAName { .. } => {
                "it passes once the name is written, whatever the code does".to_string()
            }
            Flaw::HollowSpecTest { .. } => {
                "it judges the task by tests the task writes itself".to_string()
            }
            Flaw::UnrunSpecFile { .. } => "the file holds the task to nothing".to_string(),
            Flaw::ChangesASpecTest { .. } => {
                "no session may write it, so the task cannot be done as planned".to_string()
            }
            Flaw::JudgedByAnotherTasksTest { owner, .. } => {
                format!("it passes once `{owner}` is done, whatever this task does")
            }
        }
    }

    /// What the plan or its spec has to change for the flaw to go.
    pub fn fix(&self) -> &'static str {
        match self {
            Flaw::PassesByAName { .. } => "run the test that observes the behavior",
            Flaw::HollowSpecTest { .. } => {
                "run a file the spec wrote — or give the task no spec, and say why"
            }
            Flaw::UnrunSpecFile { .. } => "name it in the command of the test that runs it",
            Flaw::ChangesASpecTest { .. } => {
                "leave the test as the spec wrote it, and change the code it tests"
            }
            Flaw::JudgedByAnotherTasksTest { .. } => "judge the task by a test of its own",
        }
    }
}

impl std::fmt::Display for Flaw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Flaw::PassesByAName { task, cmd, file } => write!(
                f,
                "`{task}` is judged by `{cmd}`, which passes once a name is written in `{file}`"
            ),
            Flaw::HollowSpecTest { task, cmd } => write!(
                f,
                "the spec's test of `{task}`, `{cmd}`, runs none of the files the spec wrote"
            ),
            Flaw::UnrunSpecFile { task, path } => write!(
                f,
                "`{path}`, which the spec wrote for `{task}`, is run by none of its tests"
            ),
            Flaw::ChangesASpecTest { task, at, owner } => write!(
                f,
                "`{task}` plans to change `{at}`, a test the spec wrote for `{owner}`"
            ),
            Flaw::JudgedByAnotherTasksTest { task, cmd, owner } => write!(
                f,
                "`{task}` is judged by `{cmd}`, the spec's test of `{owner}`"
            ),
        }
    }
}

/// What holds one task, and what of its plan cannot be done.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskReview {
    pub task: TaskId,
    /// Each criterion the task is judged by, its suite guard aside.
    pub criteria: Vec<JudgedCriterion>,
    /// Each change the plan names on a file a person approved as a test,
    /// which no session may write.
    pub denied: Vec<DeniedChange>,
    /// Each file the spec wrote for the task, with the tests that run it.
    pub files: Vec<SpecFileReview>,
    /// The task's own guards — what checks what it keeps — the suite
    /// aside.
    pub guards: Vec<JudgedCriterion>,
}

/// A file the spec wrote for a task, and the commands of its tests that
/// run it: none, and the file judges nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecFileReview {
    pub path: String,
    pub run_by: Vec<String>,
}

/// One criterion a task is judged by, and what holds the task to it.
#[derive(Debug, Clone, PartialEq)]
pub struct JudgedCriterion {
    pub cmd: String,
    pub proves: Option<String>,
    pub from: HeldTo,
}

/// What holds a task to a criterion.
#[derive(Debug, Clone, PartialEq)]
pub enum HeldTo {
    /// The plan declares it, and no test of the spec runs it.
    Plan,
    /// A test the spec gives this task; which of its files it runs is the
    /// task's [`SpecFileReview`] to say.
    Spec,
    /// The plan declares it, and it runs the test the spec gives
    /// another task.
    AnotherTask { task: TaskId },
}

/// A change the plan names on a test the spec wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct DeniedChange {
    /// The change, as the plan wrote where it is.
    pub at: String,
    /// The task whose test the file is.
    pub owner: TaskId,
}
