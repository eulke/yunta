//! Names of tools served by a node's ephemeral `yunta-run` server.

use crate::ArtifactKind;

/// A known run tool. Adapters only record failures for these names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum RunTool {
    CheckArtifact,
    TaskStatus,
    Task,
    CheckTask,
    GetBlackboard,
    RequestScopeExpansion,
    PostFinding,
    UpdateFinding,
    WithdrawFinding,
    Submit(ArtifactKind),
}

impl schemars::JsonSchema for RunTool {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("RunTool")
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "enum": Self::all().into_iter().map(Self::name).collect::<Vec<_>>()
        })
    }
}

impl From<RunTool> for String {
    fn from(tool: RunTool) -> String {
        tool.name().to_string()
    }
}

impl TryFrom<String> for RunTool {
    type Error = String;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::parse(&name).ok_or_else(|| format!("unknown run tool `{name}`"))
    }
}

impl RunTool {
    pub fn all() -> Vec<Self> {
        let mut all = vec![
            Self::CheckArtifact,
            Self::PostFinding,
            Self::UpdateFinding,
            Self::WithdrawFinding,
        ];
        all.extend(
            ArtifactKind::ALL
                .into_iter()
                .filter(|kind| kind.submit_tool().is_some())
                .map(Self::Submit),
        );
        all.extend([
            Self::TaskStatus,
            Self::Task,
            Self::CheckTask,
            Self::RequestScopeExpansion,
            Self::GetBlackboard,
        ]);
        all
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::CheckArtifact => "yunta_check_artifact",
            Self::TaskStatus => "yunta_task_status",
            Self::Task => "yunta_task",
            Self::CheckTask => "yunta_check_task",
            Self::GetBlackboard => "yunta_get_blackboard",
            Self::RequestScopeExpansion => "yunta_request_scope_expansion",
            Self::PostFinding => ArtifactKind::POST_FINDING_TOOL,
            Self::UpdateFinding => ArtifactKind::UPDATE_FINDING_TOOL,
            Self::WithdrawFinding => ArtifactKind::WITHDRAW_FINDING_TOOL,
            Self::Submit(kind) => kind.submit_tool().unwrap_or_default(),
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::all().into_iter().find(|tool| tool.name() == name)
    }

    pub fn from_claude_name(name: &str) -> Option<Self> {
        let prefix = format!("mcp__{}__", crate::port::RunToolsEndpoint::SERVER_NAME);
        Self::parse(name.strip_prefix(&prefix)?)
    }
}
