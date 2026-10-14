//! The spelling a test hands to a surface that writes next steps. Each step
//! is written as the command a person types, with the run named by its
//! handle, so a golden reads what a reader reads.
//!
//! The CLI owns its own spelling, and this crate sits below the CLI, so the
//! two are written twice: the CLI's unit test asserts they agree on every
//! kind of step.

use std::sync::Arc;

use yunta_core::step::{NextStep, Spell};

/// A spelling that writes each step as the command a person types.
pub fn spell() -> Spell {
    Arc::new(|step: &NextStep| match step {
        NextStep::Status { run, node: None } => format!("yunta status {}", run.handle()),
        NextStep::Status {
            run,
            node: Some(node),
        } => format!("yunta status {} --node {node}", run.handle()),
        NextStep::Resume { run } => format!("yunta resume {}", run.handle()),
        NextStep::Cancel { run } => format!("yunta cancel {}", run.handle()),
        NextStep::Close { run } => format!("yunta close {}", run.handle()),
        NextStep::Receipt { run } => format!("yunta receipt {}", run.handle()),
        NextStep::Verify { run } => format!("yunta verify {}", run.handle()),
        NextStep::ResolveGate {
            run, option: None, ..
        } => format!("yunta resolve-gate {}", run.handle()),
        NextStep::ResolveGate {
            run,
            option: Some(option),
            text,
        } => {
            let answer = if *text { " --text \"<answer>\"" } else { "" };
            format!("yunta resolve-gate {} {option}{answer}", run.handle())
        }
        NextStep::Gc => "yunta gc".to_string(),
        NextStep::Check { workflow } => format!("yunta check {workflow}"),
    })
}
