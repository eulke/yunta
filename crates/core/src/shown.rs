//! A document an escalation shows, read from the run for the person
//! deciding: what the log names, where its view sits, and what it says.

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
    /// A test the spec gives this task, in the file it lives in.
    Spec { file: Option<String> },
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
