//! Why the mock refuses a fixture, or a claims file it is asked to keep for one.

use std::path::PathBuf;

/// Why a fixture did not become a script.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// The YAML names a variable these paths do not define, or leaves a
    /// `{{` unclosed.
    #[error("the fixture's run paths could not be resolved")]
    Paths(#[from] yunta_core::template::TemplateError),
    /// The rendered YAML is not a fixture. The document's own
    /// diagnostic is the message: it already names the key, the path
    /// and what it expected, which is what the person who wrote the
    /// fixture needs.
    #[error(transparent)]
    Shape(#[from] yunta_core::yaml::YamlError),
    /// A declared cause the outcome's message does not read as, under the
    /// CLI the fixture stands for.
    #[error("the outcome declares the cause {declared}, but its message classifies as {actual}")]
    Cause {
        declared: &'static str,
        actual: &'static str,
    },
    /// A `tool_use` names a run tool by its protocol name, which the CLI
    /// does not call it by.
    #[error(
        "a tool_use names the run tool `{tool}` by its protocol name: this CLI calls it \
         `{called}`, so write that"
    )]
    ProtocolRunTool { tool: &'static str, called: String },
    /// A limit's reset under a CLI that prints none: the session cannot
    /// report what its CLI never says.
    #[error("the outcome names a limit's reset, which this CLI never prints")]
    UnprintedReset,
    /// The claims file a mock keeps for its run is not one this fixture can
    /// continue: it cannot be read, it is not a ledger, or it claims a script
    /// the fixture does not hold.
    #[error("the claims file {} cannot be kept for this fixture: {reason}", .path.display())]
    Claims { path: PathBuf, reason: String },
}
