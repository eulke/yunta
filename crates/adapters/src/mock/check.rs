//! The steps a fixture is refused for before it is played: the ones its CLI
//! would not print. Each rule reads the naming and the order that CLI keeps,
//! so a fixture scripts what that CLI does and nothing it cannot.

use yunta_core::{RunTool, ToolNaming};

use super::fixture::{FixtureError, MockOutcome, MockStep, SessionScript};
use crate::dialect::DialectName;

/// Refuses a `tool_use` that names a run tool by its protocol name when the
/// CLI calls its tools by a prefixed name, since that CLI never makes such a
/// call.
pub(super) fn names(steps: &[MockStep], naming: ToolNaming) -> Result<(), FixtureError> {
    if naming == ToolNaming::Bare {
        return Ok(());
    }
    for step in steps {
        if let MockStep::ToolUse { name, .. } = step {
            if let Some(tool) = RunTool::parse(name) {
                return Err(FixtureError::ProtocolRunTool {
                    tool: tool.name(),
                    called: tool.called(naming),
                });
            }
        }
    }
    Ok(())
}

/// Refuses a usage step that the CLI could not print where the script puts
/// it. Codex reports a turn's usage on the line that completes the turn, so
/// for codex a session's one usage step is its last step, and it is only
/// played by a session that completes.
pub(super) fn usage(dialect: DialectName, session: &SessionScript) -> Result<(), FixtureError> {
    if dialect != DialectName::Codex {
        return Ok(());
    }
    let played: Vec<&MockStep> = session
        .steps
        .iter()
        .chain(session.before_stop.iter().flat_map(|stop| &stop.steps))
        .collect();
    let usages = played
        .iter()
        .filter(|step| matches!(step, MockStep::Usage { .. }))
        .count();
    let last_is_usage = matches!(played.last(), Some(MockStep::Usage { .. }));
    let completes = matches!(session.outcome, MockOutcome::Completed { .. });
    match usages == 0 || (usages == 1 && last_is_usage && completes) {
        true => Ok(()),
        false => Err(FixtureError::UsageNotLast),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FixtureError, MockFixture};

    /// A codex turn reports its usage on the line that completes it, so a usage
    /// step is accepted only as the last step of a session that completes.
    #[test]
    fn a_codex_usage_step_is_accepted_only_as_the_last_step_of_a_completed_session() {
        let usage = "{type: usage, input_tokens: 1, output_tokens: 1}";
        let note = "{type: note, text: hi}";
        let completed = "outcome: {type: completed, summary: ok}";
        let last =
            format!("dialect: codex\nsessions:\n  - steps: [{note}, {usage}]\n    {completed}");
        assert!(MockFixture::parse_without_a_run(&last).is_ok());
        let early =
            format!("dialect: codex\nsessions:\n  - steps: [{usage}, {note}]\n    {completed}");
        assert!(matches!(
            MockFixture::parse_without_a_run(&early),
            Err(FixtureError::UsageNotLast)
        ));
        let failed = format!("dialect: codex\nsessions:\n  - steps: [{usage}]\n    outcome: {{type: failed, message: boom}}");
        assert!(matches!(
            MockFixture::parse_without_a_run(&failed),
            Err(FixtureError::UsageNotLast)
        ));
    }

    /// A claude-code message carries its own usage, so the usage may come anywhere.
    #[test]
    fn a_claude_usage_step_may_come_before_other_steps() {
        let yaml = "dialect: claude-code\nsessions:\n  - steps: [{type: usage, input_tokens: 1, output_tokens: 1}, {type: note, text: hi}]\n    outcome: {type: completed, summary: ok}";
        assert!(MockFixture::parse_without_a_run(yaml).is_ok());
    }
}
