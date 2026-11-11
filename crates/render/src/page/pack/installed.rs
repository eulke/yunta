//! What `pack add` answers with once the pack is vendored: the pack, its
//! version and commit, where it is, what its own cases say, and what its
//! workflows need from the project, with what the repository was detected
//! to answer for each.

use surco::components::{Fields, Furrow, Headline, Said};
use surco::doc::{Block, Doc};
use surco::look::Look;
use yunta_core::{CommitSha, ConfigKey, Detected, NodeId, PackManifest};

use super::Cases;
use crate::declare;
use crate::page::Page;

/// What an install answers with: the pack, its version and commit, where it
/// is, what its own cases say, and what it needs from this project.
pub struct Installed<'a> {
    pub manifest: &'a PackManifest,
    pub commit: &'a CommitSha,
    /// Where the pack is vendored, as a reader in this directory names it.
    pub at: &'a str,
    pub cases: &'a Cases,
    pub needs: &'a Needs,
}

impl Page for Installed<'_> {
    /// The page: installed, unless one of the pack's own cases failed,
    /// which is worth a look.
    fn document(&self, _look: &Look) -> Doc {
        let pack = format!("pack {}", self.manifest.reference());
        let headline = Headline::new(self.cases.mark(), pack, "installed")
            .fact(self.manifest.version.to_string())
            .fact(self.commit.abbreviated());
        Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with(Fields::new().push_if("at", self.at))
            .with_all(self.cases.blocks())
            .with_all(self.needs.blocks())
    }
}

/// What a pack's workflows need from this project and its config does not
/// declare, and what the repository was detected to have, which may answer
/// some of it.
#[derive(Debug, Default)]
pub struct Needs {
    /// Each need, once.
    pub unmet: Vec<Need>,
    pub detected: Detected,
}

/// What a node of an installed pack needs from this project and its config
/// does not declare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Need {
    pub node: NodeId,
    pub key: ConfigKey,
    /// Whether a run leaves the node out until the project declares it; a
    /// run is refused for it otherwise.
    pub leaves_node_out: bool,
}

impl Needs {
    /// What a pack's workflows need from this project, under a heading of
    /// its own: a line for each, saying whether a run is refused for it or
    /// leaves its node out, and under it what this repository answers for
    /// it. Each line once, so what two needs share is said under the first.
    /// Nothing when they need nothing.
    fn blocks(&self) -> Vec<Block> {
        let mut lines: Vec<String> = Vec::new();
        for need in &self.unmet {
            let fate = match need.leaves_node_out {
                true => "a run leaves the node out until it does",
                false => "a run is refused until it does",
            };
            let said = format!(
                "node `{}`: the project declares {}; {fate}",
                need.node,
                need.key.undeclared()
            );
            let detected = declare::as_detected(&self.detected, &need.key)
                .map(|detected| format!("  {detected}"));
            for line in std::iter::once(said).chain(detected) {
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
        }
        match lines.is_empty() {
            true => Vec::new(),
            false => vec![
                Block::Heading("needs from this project".to_string()),
                Said(lines).into(),
            ],
        }
    }
}
