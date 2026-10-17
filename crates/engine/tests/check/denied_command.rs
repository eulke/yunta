//! A bash command a denied pattern names: `check` refuses it, and says which rule.

use super::*;
use yunta_core::events::DeniedBy;

#[test]
fn a_bash_command_matching_a_denied_pattern_is_a_check_error_citing_the_rule() {
    let wf = workflow(vec![bash("escalate", "sudo make install", &[])]);
    let errors = check(&wf, &config_with_denied(&["sudo *"]));
    assert!(
        errors.iter().any(|e| matches!(
            e,
            CheckError::CommandDenied { node, denied }
                if node.as_str() == "escalate"
                    && matches!(&denied.by, DeniedBy::Deny { pattern } if pattern == "sudo *")
        )),
        "expected a CommandDenied error citing the pattern, got {errors:?}"
    );
}
