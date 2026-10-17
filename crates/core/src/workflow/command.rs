//! `run:` — the command a `bash` node or a hook step runs: a script
//! written into the workflow, or a command the project declares under
//! `commands:` and the workflow names.
//!
//! Naming one is how a workflow asks for a capability — "the project's
//! lint" — without knowing the tool that answers it: the name is the
//! workflow's, the text is the project's.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::parse::{describe, nested};
use super::{Node, NodeKind};
use crate::config::ConfigLayer;
use crate::ids::CommandName;
use crate::yaml::Value;

/// A command to run. The distinction is structural: a scalar is always a
/// script, and only an explicit `{ command: <name> }` names one of the
/// project's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunCommand {
    /// Text the workflow wrote, rendered with its templates before it
    /// runs.
    Script(String),
    /// A command the project declares under `commands:`, run as the
    /// project wrote it.
    Project(CommandName),
}

/// A command with the text it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved<'a> {
    Script(&'a str),
    Project {
        name: &'a CommandName,
        text: &'a str,
    },
}

impl RunCommand {
    /// The text this command runs under `config`, or the name the
    /// project does not declare.
    pub fn resolve<'a>(&'a self, config: &'a ConfigLayer) -> Result<Resolved<'a>, &'a CommandName> {
        match self {
            RunCommand::Script(text) => Ok(Resolved::Script(text)),
            RunCommand::Project(name) => config
                .command(name)
                .map(|text| Resolved::Project { name, text })
                .ok_or(name),
        }
    }

    /// The text this command runs under `config`, when there is one to
    /// read: what a check inspects before anything runs.
    pub fn text<'a>(&'a self, config: &'a ConfigLayer) -> Option<&'a str> {
        self.resolve(config).ok().map(Resolved::text)
    }

    /// The script the workflow wrote, if this is one: the only text a
    /// workflow's templates reach.
    pub fn script(&self) -> Option<&str> {
        match self {
            RunCommand::Script(text) => Some(text),
            RunCommand::Project(_) => None,
        }
    }

    /// The project's command this names, if it names one.
    pub fn project(&self) -> Option<&CommandName> {
        match self {
            RunCommand::Script(_) => None,
            RunCommand::Project(name) => Some(name),
        }
    }
}

impl<'a> Resolved<'a> {
    pub fn text(self) -> &'a str {
        match self {
            Resolved::Script(text) | Resolved::Project { text, .. } => text,
        }
    }
}

/// Every command `node` runs itself, in the order a run meets them: its
/// `before` hooks, its own command, its `after` hooks. Hooks a workflow
/// declares for every node (`node_defaults`) are the workflow's, not the
/// node's, and are not among them.
pub fn node_commands(node: &Node) -> impl Iterator<Item = &RunCommand> {
    let (before, after) = match &node.hooks {
        Some(hooks) => (hooks.before.as_slice(), hooks.after.as_slice()),
        None => (&[][..], &[][..]),
    };
    let own = match &node.kind {
        NodeKind::Bash { run } => Some(run),
        _ => None,
    };
    before
        .iter()
        .map(|step| &step.run)
        .chain(own)
        .chain(after.iter().map(|step| &step.run))
}

impl From<&str> for RunCommand {
    fn from(text: &str) -> Self {
        RunCommand::Script(text.to_string())
    }
}

impl fmt::Display for RunCommand {
    /// The script as written, or which of the project's commands it is.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunCommand::Script(text) => f.write_str(text),
            RunCommand::Project(name) => write!(f, "the project's command `{name}`"),
        }
    }
}

impl<'de> Deserialize<'de> for RunCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Named {
            command: CommandName,
        }

        match Value::deserialize(deserializer)? {
            Value::String(text) => Ok(RunCommand::Script(text)),
            mapping @ Value::Mapping(_) => {
                let Named { command } = nested::<D, _>("run", mapping)?;
                Ok(RunCommand::Project(command))
            }
            other => Err(D::Error::custom(format!(
                "a command is a script or `{{ command: <name> }}`, not {}",
                describe(&other)
            ))),
        }
    }
}

impl Serialize for RunCommand {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;

        match self {
            RunCommand::Script(text) => serializer.serialize_str(text),
            RunCommand::Project(name) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("command", name)?;
                map.end()
            }
        }
    }
}

impl schemars::JsonSchema for RunCommand {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RunCommand".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A script, or `{ command: <name> }` to run a command the project declares under `commands:`",
            "oneOf": [
                { "type": "string" },
                {
                    "type": "object",
                    "properties": { "command": { "type": "string" } },
                    "required": ["command"],
                    "additionalProperties": false
                }
            ]
        })
    }
}
