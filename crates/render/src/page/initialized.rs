//! What `yunta init` did and found, as a person reads it: the files it wrote
//! and the ones it left, the ecosystem and the forge it detected, each
//! adapter this machine answers for, the lines for the project's CLAUDE.md,
//! and the runner to declare, since no agent node runs until one is.

use std::path::Path;

use surco::components::{Checklist, Fields, Found, Furrow, Headline, Next, Said};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::NextStep;
use yunta_core::{Detected, MachineAdapters, ProbedAdapter};

use super::Page;
use crate::{command, declare};

/// What `init` did and found, as the page it ends with.
pub struct Initialized<'a> {
    /// The repository `init` prepared.
    pub repo: &'a Path,
    /// What it wrote, by its path in the repository.
    pub wrote: &'a [String],
    /// What it left as it was, and why.
    pub skipped: &'a [String],
    /// What the repository was found to have.
    pub detected: &'a Detected,
    /// Every adapter this build knows, probed.
    pub probed: &'a [ProbedAdapter],
}

impl Page for Initialized<'_> {
    /// The page `init` ends with: what it wrote and detected, as fields; each
    /// adapter this machine answers for, as a check; the lines to paste into
    /// the project's CLAUDE.md; and the runner to declare, because no agent
    /// node runs until one is.
    fn document(&self, _look: &Look) -> Doc {
        let mut adapters = Checklist::default();
        for adapter in self.probed {
            match adapter.healthy {
                true => adapters.push(
                    Found::Holds,
                    adapter.id.as_str(),
                    format!("healthy ({})", adapter.detail),
                ),
                false => adapters.push(
                    Found::Caution,
                    adapter.id.as_str(),
                    format!("unavailable ({})", adapter.detail),
                ),
            }
        }
        let suggestion = claude_md_suggestion();
        // The config is written with its runners commented out: a probe
        // names an adapter and never a model, and the model is the project's
        // to choose. Until a runner is declared, no agent node can run, so
        // that is the step this ends with.
        let runner = declare::runners(&[], true, &MachineAdapters::probed(self.probed));
        let headline = Headline::new(Mark::Done, command::line(&NextStep::Init), "done")
            .fact(format!("in {}", self.repo.display()));
        Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with(detected_fields(self))
            .with(adapters)
            .with(Block::Heading(
                "for this repo's CLAUDE.md, to paste yourself: Yunta never writes to that file"
                    .to_string(),
            ))
            .with(Said(suggestion))
            .with(Block::Heading(
                "no runner is declared, so a workflow's agent nodes cannot run until one is"
                    .to_string(),
            ))
            .with_all(runner.blocks())
            .with(Next {
                steps: vec![(
                    command::line(&NextStep::Doctor {
                        sessions: false,
                        full: false,
                    }),
                    "confirms everything above is usable",
                )],
            })
    }
}

/// What `init` wrote and detected, as the fields the page opens with: the
/// files it wrote and the ones it left as they were, the ecosystem it found
/// and its cache tip, and the forge `origin` is on.
fn detected_fields(init: &Initialized<'_>) -> Fields {
    let detected = init.detected;
    let ecosystem = match &detected.ecosystem {
        Some(ecosystem) => format!(
            "{}: wrote {}{}",
            ecosystem.name,
            surco::text::counted(detected.commands.len(), "project command"),
            match detected.suite {
                Some(_) => " and the suite a run measures",
                None => "",
            }
        ),
        None => "none known (looked for Cargo.toml, package.json, go.mod, pyproject.toml); \
                 declare `commands:` and `baseline.suite` by hand"
            .to_string(),
    };
    Fields::new()
        .push_if("wrote", init.wrote.join(", "))
        .push_if("skipped", init.skipped.join(", "))
        .push_if("ecosystem", ecosystem)
        .push_if(
            "tip",
            detected
                .ecosystem
                .as_ref()
                .map(|ecosystem| ecosystem.cache_tip.to_string())
                .unwrap_or_default(),
        )
        .push_if(
            "forge",
            detected
                .forge
                .as_ref()
                .map(|repo| {
                    format!("github {repo}, from `origin`; its token is read from `GITHUB_TOKEN`")
                })
                .unwrap_or_default(),
        )
}

/// What this repository's CLAUDE.md could say, a line at a time as a
/// person pastes it.
fn claude_md_suggestion() -> Vec<String> {
    vec![
        "This repo uses Yunta for verified workflows. Before implementing".to_string(),
        format!(
            "something ad hoc, check `{}` (or the `list_workflows` MCP",
            command::line(&NextStep::List)
        ),
        "tool) for an existing verified workflow that already covers it.".to_string(),
    ]
}
