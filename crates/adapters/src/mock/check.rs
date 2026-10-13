//! The steps a fixture is refused for before it is played: the ones its CLI
//! would not print. The rule reads the naming that CLI keeps, so a fixture
//! scripts what that CLI does and nothing it cannot.

use yunta_core::{RunTool, ToolNaming};

use super::fixture::{FixtureError, MockStep};

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
