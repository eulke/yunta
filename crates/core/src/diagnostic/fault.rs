//! What a rule found wrong with a document, as the values its clause names.
//!
//! Each family of rules keeps its own enum beside its rules. A broken rule
//! holds the [`Fault`] of its family; the clause a reader acts on is written
//! from it wherever the report is shown or sent, and the log keeps the value.

use serde::{Deserialize, Serialize};

use super::{RuleCode, RunFault};
use crate::findings::fault::FindingFault;
use crate::questions::QuestionFault;
use crate::spec::SpecFault;
use crate::tasks::TaskFault;
use crate::workflow::WorkflowFault;

/// What a rule found wrong with a document, as the values its clause names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    Tasks(TaskFault),
    Spec(SpecFault),
    Questions(QuestionFault),
    Findings(FindingFault),
    Workflow(WorkflowFault),
    /// What the engine found running a document's commands where it runs them.
    Run(RunFault),
}

impl Fault {
    /// The stable name of the rule this fault breaks: what a receipt counts.
    pub fn code(&self) -> RuleCode {
        match self {
            Fault::Tasks(fault) => fault.code(),
            Fault::Spec(fault) => fault.code(),
            Fault::Questions(fault) => fault.code(),
            Fault::Findings(fault) => fault.code(),
            Fault::Workflow(fault) => fault.code(),
            Fault::Run(fault) => fault.code(),
        }
    }

    /// The clause a reader acts on: what the rule found, in words.
    pub fn clause(&self) -> String {
        match self {
            Fault::Tasks(fault) => fault.clause(),
            Fault::Spec(fault) => fault.clause(),
            Fault::Questions(fault) => fault.clause(),
            Fault::Findings(fault) => fault.clause(),
            Fault::Workflow(fault) => fault.clause(),
            Fault::Run(fault) => fault.clause(),
        }
    }
}

impl From<TaskFault> for Fault {
    fn from(fault: TaskFault) -> Self {
        Fault::Tasks(fault)
    }
}

impl From<SpecFault> for Fault {
    fn from(fault: SpecFault) -> Self {
        Fault::Spec(fault)
    }
}

impl From<QuestionFault> for Fault {
    fn from(fault: QuestionFault) -> Self {
        Fault::Questions(fault)
    }
}

impl From<FindingFault> for Fault {
    fn from(fault: FindingFault) -> Self {
        Fault::Findings(fault)
    }
}

impl From<WorkflowFault> for Fault {
    fn from(fault: WorkflowFault) -> Self {
        Fault::Workflow(fault)
    }
}

impl From<RunFault> for Fault {
    fn from(fault: RunFault) -> Self {
        Fault::Run(fault)
    }
}
