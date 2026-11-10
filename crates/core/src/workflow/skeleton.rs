//! The shapes a new workflow starts as, each a skeleton `yunta new` writes.

use crate::refusal::SetupRefusal;

/// A shape a new workflow starts as. Not a document's shape (`crate::shape`), which
/// says what a document holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowShape {
    OneNode,
    LintFix,
    Tasks,
}

impl WorkflowShape {
    /// The shape named `name`, or refused naming the ones that exist. Parsed by
    /// [`WorkflowShape::label`], the same word a menu shows and a page reports, so a
    /// shape cannot be spelled one way to a reader and another to the parser.
    pub fn parse(name: &str) -> Result<Self, SetupRefusal> {
        Self::all()
            .into_iter()
            .find(|shape| shape.label() == name)
            .ok_or_else(|| SetupRefusal::UnknownShape {
                name: name.to_string(),
                shapes: Self::all()
                    .iter()
                    .map(|shape| shape.label().to_string())
                    .collect(),
            })
    }

    /// Every shape, the plainest first.
    pub fn all() -> [Self; 3] {
        [Self::OneNode, Self::LintFix, Self::Tasks]
    }

    /// The word the shape is named by.
    pub fn label(&self) -> &'static str {
        match self {
            Self::OneNode => "one-node",
            Self::LintFix => "lint-fix",
            Self::Tasks => "tasks",
        }
    }
}
