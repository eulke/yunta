//! A named agent: instructions a session works under beside its CLI's own.
//!
//! A node or a runner names an agent with `agent:`, and the run resolves
//! the name to a definition it owns — a Markdown file whose front matter
//! names it and says what it is for, and whose body is the instructions.
//! Every adapter hands the instructions to its CLI the same way, added to
//! the CLI's own system instructions; the model is the runner's to choose,
//! because a model belongs to one provider, and the tools are the
//! permission profile's.

use serde::Deserialize;
use thiserror::Error;

use crate::AgentName;

/// An agent as a session receives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDefinition {
    pub name: AgentName,
    /// What the session works under, added to its CLI's own instructions.
    pub instructions: String,
}

/// What an agent's front matter says.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrontMatter {
    name: AgentName,
    /// What the agent is for: required of every definition, read by people.
    #[serde(rename = "description")]
    _description: String,
}

/// Why a file is not the definition of the agent it was looked up as.
#[derive(Debug, Error)]
pub enum AgentDefinitionError {
    #[error(
        "agent `{name}`: the file opens with no front matter — start it with `---`, its `name` \
         and `description`, and `---`"
    )]
    NoFrontMatter { name: AgentName },
    #[error(
        "agent `{name}`: its front matter does not read — it holds `name` and `description` \
         only; the model is the runner's and the tools are the permission profile's"
    )]
    FrontMatter {
        name: AgentName,
        #[source]
        source: crate::yaml::YamlError,
    },
    #[error("agent `{name}`: the file names the agent `{named}`")]
    NamedOtherwise { name: AgentName, named: AgentName },
    #[error("agent `{name}`: the file gives no instructions below its front matter")]
    NoInstructions { name: AgentName },
}

impl AgentDefinition {
    /// Reads `text` as the definition of `name`.
    pub fn parse(name: &AgentName, text: &str) -> Result<Self, AgentDefinitionError> {
        let no_front_matter = || AgentDefinitionError::NoFrontMatter { name: name.clone() };
        let rest = text
            .strip_prefix("---\n")
            .or_else(|| text.strip_prefix("---\r\n"))
            .ok_or_else(no_front_matter)?;
        let (front, body) = rest
            .split_once("\n---\n")
            .or_else(|| rest.split_once("\n---\r\n"))
            .or_else(|| rest.strip_suffix("\n---").map(|front| (front, "")))
            .ok_or_else(no_front_matter)?;
        let front: FrontMatter =
            crate::yaml::parse(front).map_err(|source| AgentDefinitionError::FrontMatter {
                name: name.clone(),
                source,
            })?;
        if &front.name != name {
            return Err(AgentDefinitionError::NamedOtherwise {
                name: name.clone(),
                named: front.name,
            });
        }
        let instructions = body.trim();
        if instructions.is_empty() {
            return Err(AgentDefinitionError::NoInstructions { name: name.clone() });
        }
        Ok(AgentDefinition {
            name: name.clone(),
            instructions: instructions.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name() -> AgentName {
        "reviewer".parse().unwrap()
    }

    #[test]
    fn a_definition_is_its_name_and_the_instructions_below_its_front_matter() {
        let text =
            "---\nname: reviewer\ndescription: Reviews a change.\n---\n\nRead the diff first.\n";
        let agent = AgentDefinition::parse(&name(), text).unwrap();
        assert_eq!(agent.instructions, "Read the diff first.");
    }

    #[test]
    fn a_model_or_tools_in_the_front_matter_are_refused() {
        let text = "---\nname: reviewer\ndescription: d\nmodel: opus\n---\nx\n";
        assert!(matches!(
            AgentDefinition::parse(&name(), text),
            Err(AgentDefinitionError::FrontMatter { .. })
        ));
    }

    #[test]
    fn a_file_without_front_matter_or_instructions_is_no_definition() {
        assert!(matches!(
            AgentDefinition::parse(&name(), "Read the diff.\n"),
            Err(AgentDefinitionError::NoFrontMatter { .. })
        ));
        assert!(matches!(
            AgentDefinition::parse(&name(), "---\nname: reviewer\ndescription: d\n---\n\n"),
            Err(AgentDefinitionError::NoInstructions { .. })
        ));
    }

    #[test]
    fn a_file_naming_another_agent_is_not_this_one() {
        assert!(matches!(
            AgentDefinition::parse(&name(), "---\nname: other\ndescription: d\n---\nx\n"),
            Err(AgentDefinitionError::NamedOtherwise { .. })
        ));
    }
}
