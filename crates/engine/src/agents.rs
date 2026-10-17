//! Agent-name resolution: `agent:` on a node or a runner, resolved to a
//! definition the run owns. A project's own agents live under
//! `.yunta/agents/<name>.md`; a pack's are named `publisher/agent` and
//! found among the pack's declared `contents.agents`, the same layering
//! skills follow: a project file at the same path wins over the pack's.
//! The walk that freezes a run reads these names once, at birth; a node of
//! a run that predates frozen sets reads them from the checkout, through
//! the same lookup. Handing the instructions to the CLI is the adapter's.

use std::path::{Path, PathBuf};

use yunta_core::agent::{AgentDefinition, AgentDefinitionError};
use yunta_core::{AgentName, Publisher};

use crate::catalog::{packs_for_publisher, DeclaredPath};

/// Where a project keeps its own agents, relative to its checkout.
const PROJECT_AGENTS: &str = ".yunta/agents";

/// An agent name that resolves to no definition, or to one that does not
/// read.
#[derive(Debug, thiserror::Error)]
pub enum AgentsError {
    #[error(
        "agent `{name}` not found at `{}` — write its definition there, or install the pack \
         that ships it",
        searched.display()
    )]
    NotFound { name: AgentName, searched: PathBuf },
    #[error("agent `{name}` could not be read from `{}`", path.display())]
    Unreadable {
        name: AgentName,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Definition(#[from] AgentDefinitionError),
}

/// Where a name's definition is: the project's own file, or the file a
/// pack declares.
pub(crate) enum Location {
    Project(PathBuf),
    Pack(DeclaredPath),
}

/// The file `name` names in the checkout at `worktree`: the project's own
/// file at its path, else the one pack file that declares the name.
pub(crate) fn locate(worktree: &Path, name: &AgentName) -> Result<Location, AgentsError> {
    let project = worktree.join(PROJECT_AGENTS).join(format!("{name}.md"));
    if project.is_file() {
        return Ok(Location::Project(project));
    }
    pack_agent(worktree, name.as_str())
        .map(Location::Pack)
        .ok_or_else(|| AgentsError::NotFound {
            name: name.clone(),
            searched: worktree.join(PROJECT_AGENTS),
        })
}

/// The definition `name` is, as the text of its file reads: the file's own
/// name is the bare one, and the definition keeps the name it was looked up
/// as.
pub(crate) fn definition(name: &AgentName, text: &str) -> Result<AgentDefinition, AgentsError> {
    let bare = name
        .as_str()
        .rsplit('/')
        .next()
        .and_then(|bare| bare.parse::<AgentName>().ok())
        .unwrap_or_else(|| name.clone());
    let definition = AgentDefinition::parse(&bare, text)?;
    Ok(AgentDefinition {
        name: name.clone(),
        ..definition
    })
}

/// `publisher/agent`: the one file among a publisher's installed packs'
/// declared agents whose stem is `agent` — none when two packs ship one.
fn pack_agent(worktree: &Path, name: &str) -> Option<DeclaredPath> {
    let (publisher, agent) = name.split_once('/')?;
    let publisher = publisher.parse::<Publisher>().ok()?;
    let mut found: Option<DeclaredPath> = None;
    for (pack_dir, manifest) in packs_for_publisher(worktree, &publisher).installed {
        for declared in &manifest.contents.agents {
            let stem = Path::new(declared)
                .file_stem()
                .and_then(|stem| stem.to_str());
            if stem == Some(agent) {
                if found.is_some() {
                    return None;
                }
                found = Some(DeclaredPath::of_pack(
                    pack_dir.clone(),
                    &manifest,
                    declared.clone(),
                ));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frozen::read::Source;
    use yunta_core::ConfigLayer;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn definition(name: &str) -> String {
        format!("---\nname: {name}\ndescription: d\n---\nWork as {name}.\n")
    }

    fn pack(root: &Path, pack: &str, agents: &[&str]) {
        let dir = root.join(".yunta/packs/acme").join(pack);
        write(
            &dir.join("pack.yaml"),
            &format!(
                "name: {pack}\npublisher: acme\nversion: 1.0.0\ndeclares:\n  permissions: read-only\n\
                 contents:\n  agents: [{}]\n",
                agents.join(", ")
            ),
        );
        for agent in agents {
            let stem = Path::new(agent).file_stem().unwrap().to_str().unwrap();
            write(&dir.join(agent), &definition(stem));
        }
    }

    fn name(name: &str) -> AgentName {
        name.parse().unwrap()
    }

    /// The definition a checkout at `root` gives `agent`, read the way a run
    /// that predates frozen sets reads it.
    async fn checkout_agent(
        root: &Path,
        agent: &str,
    ) -> Result<AgentDefinition, crate::frozen::FrozenError> {
        let config = ConfigLayer::default();
        let source = Source::Checkout {
            worktree: root,
            config: &config,
            ambient: None,
        };
        source.agent(&name(agent)).await
    }

    #[tokio::test]
    async fn a_project_agent_is_its_file_under_the_project_s_agents() {
        let root = tempfile::tempdir().unwrap();
        write(
            &root.path().join(".yunta/agents/reviewer.md"),
            &definition("reviewer"),
        );
        let agent = checkout_agent(root.path(), "reviewer").await.unwrap();
        assert_eq!(agent.instructions, "Work as reviewer.");
    }

    #[tokio::test]
    async fn a_pack_agent_is_the_declared_file_named_like_it_and_keeps_its_full_name() {
        let root = tempfile::tempdir().unwrap();
        pack(root.path(), "review", &["agents/auditor.md"]);
        let agent = checkout_agent(root.path(), "acme/auditor").await.unwrap();
        assert_eq!(agent.name, name("acme/auditor"));
        assert_eq!(agent.instructions, "Work as auditor.");
    }

    #[tokio::test]
    async fn a_project_file_at_a_pack_agent_s_path_wins_over_the_pack() {
        let root = tempfile::tempdir().unwrap();
        pack(root.path(), "review", &["auditor.md"]);
        write(
            &root.path().join(".yunta/agents/acme/auditor.md"),
            "---\nname: auditor\ndescription: d\n---\nThe project's own.\n",
        );
        let agent = checkout_agent(root.path(), "acme/auditor").await.unwrap();
        assert_eq!(agent.instructions, "The project's own.");
    }

    #[tokio::test]
    async fn two_packs_of_one_publisher_shipping_one_name_resolve_to_neither() {
        let root = tempfile::tempdir().unwrap();
        pack(root.path(), "one", &["auditor.md"]);
        pack(root.path(), "two", &["auditor.md"]);
        assert!(matches!(
            checkout_agent(root.path(), "acme/auditor").await,
            Err(crate::frozen::FrozenError::Unresolved(_))
        ));
    }
}
