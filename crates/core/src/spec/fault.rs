//! What a spec document's rules found wrong with it, as the values each
//! clause names. The clause is what a reader acts on; the code is the stable
//! name a receipt counts the rule by.

use serde::{Deserialize, Serialize};

use crate::diagnostic::RuleCode;

/// What a rule of a spec document found wrong with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SpecFault {
    DuplicateSpec,
    NoSpecTest,
    UnexplainedTest { cmd: String },
    TestFileEscapes { path: String },
    DuplicateTestFile { path: String },
    UnrunSpecFile { path: String },
    SpecTestRunsNoSpecFile { cmd: String },
    OutsideTheSuite { command: String, suite: String },
}

impl SpecFault {
    /// The stable name of the rule this fault breaks.
    pub fn code(&self) -> RuleCode {
        match self {
            Self::DuplicateSpec => RuleCode::DuplicateSpec,
            Self::NoSpecTest => RuleCode::NoSpecTest,
            Self::UnexplainedTest { .. } => RuleCode::UnexplainedTest,
            Self::TestFileEscapes { .. } => RuleCode::TestFileEscapes,
            Self::DuplicateTestFile { .. } => RuleCode::DuplicateTestFile,
            Self::UnrunSpecFile { .. } => RuleCode::UnrunSpecFile,
            Self::SpecTestRunsNoSpecFile { .. } => RuleCode::SpecTestRunsNoSpecFile,
            Self::OutsideTheSuite { .. } => RuleCode::OutsideTheSuite,
        }
    }

    /// The clause a reader acts on, in the words the rule has always been
    /// written in.
    pub fn clause(&self) -> String {
        match self {
            Self::DuplicateSpec => {
                "a second spec already names this task; give each task one".to_string()
            }
            Self::NoSpecTest => {
                "lists no test; a spec is the tests its task is held to".to_string()
            }
            Self::UnexplainedTest { cmd } => {
                format!("`{cmd}` does not say what passing it proves")
            }
            Self::TestFileEscapes { path } => format!(
                "`{path}` is not a file of the repository: a path is relative, never climbs out, \
                 and never reaches into `.git`"
            ),
            Self::DuplicateTestFile { path } => {
                format!("`{path}` is named by a second file; each file is named once")
            }
            Self::UnrunSpecFile { path } => format!(
                "`{path}` is run by none of this task's tests; name it in the command that runs it"
            ),
            Self::SpecTestRunsNoSpecFile { cmd } => format!(
                "`{cmd}` runs none of the files this spec writes, so it judges the task by tests \
                 the task writes itself; run a file of the spec — and if no test can observe the \
                 change through what the plan declares, give the task no spec and say so"
            ),
            Self::OutsideTheSuite { command, suite } => {
                crate::tasks::outside_clause(command, suite)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SpecFault;

    #[test]
    fn each_clause_reads_as_the_spec_has_always_said_it() {
        assert_eq!(
            SpecFault::NoSpecTest.clause(),
            "lists no test; a spec is the tests its task is held to"
        );
        assert_eq!(
            SpecFault::TestFileEscapes {
                path: "../x.sh".to_string(),
            }
            .clause(),
            "`../x.sh` is not a file of the repository: a path is relative, never climbs out, and \
             never reaches into `.git`"
        );
        assert_eq!(
            SpecFault::UnrunSpecFile {
                path: "tests/a.sh".to_string(),
            }
            .clause(),
            "`tests/a.sh` is run by none of this task's tests; name it in the command that runs it"
        );
        assert_eq!(
            SpecFault::OutsideTheSuite {
                command: "pytest tests/a.py".to_string(),
                suite: "pytest -q".to_string(),
            }
            .clause(),
            "`pytest tests/a.py` runs the project's tests outside the suite `pytest -q`, so it \
             builds them again beside the suite's build; extend the suite instead — `pytest -q \
             <selector>`, where `<selector>` picks this task's tests"
        );
    }
}
