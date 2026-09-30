//! The `kind: spec` document: the tests a plan's tasks are held to,
//! written before any task is built and by someone other than whoever
//! builds it.
//!
//! Each task gets the files its tests live in, whole, and the commands
//! that run them. The engine writes a task's files into the tree its
//! work starts from, denies that work any change to them, and adds the
//! commands to its criteria: a test the work makes pass is one it could
//! not write for itself.

use serde::{Deserialize, Serialize};

use crate::{Criterion, TaskId};

mod rules;

/// The document — sole top-level key `specs:`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpecFile {
    pub specs: Vec<Spec>,
}

impl SpecFile {
    /// The spec of `task`, when the document has one.
    pub fn of(&self, task: &TaskId) -> Option<&Spec> {
        self.specs.iter().find(|spec| &spec.task == task)
    }
}

/// What one task is held to: the files its tests live in, and the tests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub task: TaskId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<TestFile>,
    pub tests: Vec<SpecTest>,
}

impl Spec {
    /// The tests as the task's criteria: none of them a guard, each
    /// saying what it proves.
    pub fn criteria(&self) -> impl Iterator<Item = Criterion> + '_ {
        self.tests.iter().map(|test| Criterion {
            cmd: test.cmd.clone(),
            r#type: None,
            proves: Some(test.proves.clone()),
        })
    }
}

/// A file a task's tests live in, whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TestFile {
    pub path: String,
    pub content: String,
}

/// One test: the command that runs it, and what its passing proves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpecTest {
    pub cmd: String,
    pub proves: String,
}

/// The shape this document publishes, as the YAML it is.
const EXAMPLE: &str = include_str!("shape.yaml");

impl crate::shape::Document for SpecFile {
    const KIND: crate::ArtifactKind = crate::ArtifactKind::Spec;
    const EXAMPLE: &'static str = EXAMPLE;

    fn check(&self) -> Vec<crate::diagnostic::Diagnostic> {
        rules::check(self)
    }

    const RULES: &'static [crate::diagnostic::Rule] = rules::RULES;

    const RUN_RULES: &'static [crate::diagnostic::Rule] = rules::RUN_RULES;
}
