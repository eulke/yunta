//! See [`super`]. One family of workflow-check rules.

use super::*;

/// Each declared input's own fields are internally consistent
/// — independent of anything else in the workflow, so this runs once
/// over `inputs:` rather than per reference site.
pub(crate) fn check_input_specs(
    inputs: &std::collections::BTreeMap<String, InputSpec>,
    errors: &mut Vec<CheckError>,
) {
    for (name, spec) in inputs {
        match spec {
            InputSpec::Enum { values, .. } if values.is_empty() => {
                errors.push(CheckError::InputEmptyEnumValues { name: name.clone() });
            }
            InputSpec::Number {
                min: Some(min),
                max: Some(max),
                ..
            } if min > max => {
                errors.push(CheckError::InputMinExceedsMax {
                    name: name.clone(),
                    min: min.to_string(),
                    max: max.to_string(),
                });
            }
            InputSpec::String {
                pattern: Some(pattern),
                ..
            } => {
                if let Err(e) = regex::Regex::new(pattern) {
                    errors.push(CheckError::InputInvalidPattern {
                        name: name.clone(),
                        pattern: pattern.clone(),
                        detail: e.to_string(),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Every `{{inputs.x}}` appearing in an inline template must name
/// a declared input. Scans exactly the text the runtime ever
/// renders (`node_exec.rs`/`context_resolve.rs`'s own `render_template`
/// call sites) — prompt text, bash/hook commands, and `context:`
/// patterns/command/query — so a reference `check` accepts is guaranteed
/// renderable and vice versa.
pub(crate) fn check_input_references(workflow: &Workflow, errors: &mut Vec<CheckError>) {
    if let Some(defaults) = &workflow.node_defaults {
        if let Some(hooks) = &defaults.hooks {
            for step in hooks.before.iter().chain(&hooks.after) {
                check_template_text(&NODE_DEFAULTS, &step.run, workflow, errors);
            }
        }
    }
    check_input_references_in_nodes(&workflow.nodes, workflow, errors);
}

pub(crate) fn check_input_references_in_nodes(
    nodes: &[Node],
    workflow: &Workflow,
    errors: &mut Vec<CheckError>,
) {
    for node in nodes {
        match &node.kind {
            NodeKind::Prompt {
                prompt: yunta_core::PromptSource::Inline(text),
            } => check_template_text(&node.id, text, workflow, errors),
            NodeKind::Bash { run } => check_template_text(&node.id, run, workflow, errors),
            NodeKind::Loop {
                prompt: yunta_core::PromptSource::Inline(text),
                ..
            } => check_template_text(&node.id, text, workflow, errors),
            NodeKind::Parallel {
                nodes: children, ..
            } => {
                check_input_references_in_nodes(children, workflow, errors);
            }
            _ => {}
        }

        if let Some(hooks) = &node.hooks {
            for step in hooks.before.iter().chain(&hooks.after) {
                check_template_text(&node.id, &step.run, workflow, errors);
            }
        }

        for source in &node.context {
            match source {
                yunta_core::ContextSpec::Files { files } => {
                    for pattern in files {
                        check_template_text(&node.id, pattern, workflow, errors);
                    }
                }
                yunta_core::ContextSpec::Command { command } => {
                    check_template_text(&node.id, command, workflow, errors);
                }
                yunta_core::ContextSpec::Mcp { mcp } => {
                    check_template_text(&node.id, &mcp.query, workflow, errors);
                }
                _ => {}
            }
        }
    }
}

pub(crate) fn check_template_text(
    node: &NodeId,
    text: &str,
    workflow: &Workflow,
    errors: &mut Vec<CheckError>,
) {
    let Ok(variables) = template_variables(text) else {
        // An unclosed `{{` is a template-syntax error, not an inputs
        // one — the runtime's own `render_template` reports that when
        // this node actually executes; nothing new to say here.
        return;
    };
    for variable in variables {
        if let Some(name) = variable.strip_prefix("inputs.") {
            if !workflow.inputs.contains_key(name) {
                errors.push(CheckError::UndeclaredInput {
                    node: node.clone(),
                    name: name.to_string(),
                });
            }
        }
    }
}
