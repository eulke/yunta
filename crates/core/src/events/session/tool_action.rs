//! The word a tool call is read by.

use serde::{Deserialize, Serialize};

/// What a tool call does, as a person reads it: one word, whichever CLI made
/// the call. It says nothing the session typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolAction {
    Read,
    Edit,
    Write,
    Search,
    List,
    Run,
    Fetch,
    Call,
    Submit,
}

impl ToolAction {
    /// The word a reader is told.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Write => "write",
            Self::Search => "search",
            Self::List => "list",
            Self::Run => "run",
            Self::Fetch => "fetch",
            Self::Call => "call",
            Self::Submit => "submit",
        }
    }

    /// What a tool does by its name alone: a CLI names most of its tools for
    /// what they do, and a log older than this word has only the name. A name
    /// this table does not hold is a call.
    pub fn of_tool(name: &str) -> Self {
        match name {
            "Read" => Self::Read,
            "Edit" | "MultiEdit" | "NotebookEdit" | "file_change" => Self::Edit,
            "Write" => Self::Write,
            "Grep" | "WebSearch" | "web_search" => Self::Search,
            "Glob" | "LS" => Self::List,
            "Bash" | "command_execution" => Self::Run,
            "WebFetch" => Self::Fetch,
            _ => crate::RunTool::parse(name).map_or(Self::Call, crate::RunTool::action),
        }
    }
}
