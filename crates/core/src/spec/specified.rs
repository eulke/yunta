//! What a spec is held to when it is handed over: its tests run the files
//! it writes. A spec whose files nothing runs, or whose tests run only
//! what the task writes itself, judges the work by nothing it wrote — and
//! a spec that does not test cannot hold a plan to anything.
//!
//! Like the plan's, these rules are asked of a spec when it is handed
//! over, never when one is read back.

use crate::diagnostic::{Diagnostic, Named, Problem, Rule, RuleCode, Subject};
use crate::SpecFile;

pub(super) const SPECIFIED_RULES: &[Rule] = &[
    Rule {
        code: RuleCode::UnrunSpecFile,
        demand: "every file a spec writes is run by one of its tests: name it in the command \
                 that runs it",
    },
    Rule {
        code: RuleCode::SpecTestRunsNoSpecFile,
        demand: "every test of a spec that writes files runs one of them: a test that runs only \
                 what the task writes judges the task by its own work",
    },
];

impl SpecFile {
    /// Every file no test of its spec runs, and every test that runs none
    /// of its spec's files.
    pub fn untested(&self) -> Vec<Diagnostic> {
        let mut broken = Vec::new();
        for (index, spec) in self.specs.iter().enumerate() {
            let subject = || Subject::Spec(Named::new(spec.task.clone(), index));
            for file in spec.unrun_files() {
                broken.push(Diagnostic::new(
                    subject(),
                    Problem::rule(
                        RuleCode::UnrunSpecFile,
                        format!(
                            "`{}` is run by none of this task's tests; name it in the command \
                             that runs it",
                            file.path
                        ),
                    ),
                ));
            }
            for test in spec.hollow_tests() {
                broken.push(Diagnostic::new(
                    subject(),
                    Problem::rule(
                        RuleCode::SpecTestRunsNoSpecFile,
                        format!(
                            "`{}` runs none of the files this spec writes, so it judges the task \
                             by tests the task writes itself; run a file of the spec — and if no \
                             test can observe the change through what the plan declares, give \
                             the task no spec and say so",
                            test.cmd
                        ),
                    ),
                ));
            }
        }
        broken
    }
}
