//! The checks of a loop's `scope_expansion` block, wherever the loop sits:
//! the retired `within` and the mode ceiling. A workflow that still declares
//! `within:` fails `check`, because the ceiling it named moved to the
//! project's `permissions.paths.deny`, and the refusal says so. A mode above
//! `permissions.scope_expansion.max_mode` fails it too.

use yunta_core::{ConfigLayer, Workflow};
use yunta_engine::{check as check_against, CheckError};

/// The rules under test here are about the workflow, not about which adapter
/// would run it: these check against a binary that builds none.
fn check(workflow: &Workflow, config: &ConfigLayer) -> Vec<CheckError> {
    check_against(workflow, config, &|_| None)
}

#[test]
fn a_workflow_declaring_within_fails_check_naming_the_project_deny() {
    let workflow = r#"
name: expansive
nodes:
  - id: implement
    kind: loop
    until: all_tasks_complete
    prompt: "work"
    scope_expansion:
      mode: rules
      within: ["src/**"]
"#;
    let wf: Workflow = yunta_core::yaml::parse(workflow).unwrap();
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, CheckError::ScopeExpansionWithinRetired { .. })),
        "{errors:?}"
    );
}

#[test]
fn a_loop_inside_a_parallel_group_declaring_within_fails_check_too() {
    let workflow = r#"
name: expansive
nodes:
  - id: group
    kind: parallel
    nodes:
      - id: implement
        kind: loop
        until: all_tasks_complete
        prompt: "work"
        scope_expansion:
          mode: rules
          within: ["src/**"]
"#;
    let wf: Workflow = yunta_core::yaml::parse(workflow).unwrap();
    let errors = check(&wf, &ConfigLayer::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::ScopeExpansionWithinRetired { node } if node.as_str() == "implement"
        )),
        "{errors:?}"
    );
}

#[test]
fn a_loop_inside_a_parallel_group_is_held_to_the_mode_ceiling() {
    let workflow = r#"
name: expansive
nodes:
  - id: group
    kind: parallel
    nodes:
      - id: implement
        kind: loop
        until: all_tasks_complete
        prompt: "work"
        scope_expansion:
          mode: rules
"#;
    let wf: Workflow = yunta_core::yaml::parse(workflow).unwrap();
    let ceiling: ConfigLayer =
        yunta_core::yaml::parse("permissions: { scope_expansion: { max_mode: ask } }").unwrap();
    let errors = check(&wf, &ceiling);
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::ScopeExpansionModeOverCeiling { node, .. } if node.as_str() == "implement"
        )),
        "{errors:?}"
    );
}
