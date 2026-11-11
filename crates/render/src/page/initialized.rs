//! What `yunta init` did and found, as a person reads it: the files it wrote
//! and the ones it left, the ecosystem and the forge it detected, each
//! adapter this machine answers for, the lines for the project's CLAUDE.md,
//! the runners a person declared, and then either a pack of workflows to
//! install, where every runner is declared, or the runners to declare, since
//! no agent node runs until its runner is.

use std::collections::BTreeMap;
use std::path::Path;

use surco::components::{Checklist, Fields, Found, Furrow, Headline, Next, Said};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::{Arg, NextStep};
use yunta_core::{Detected, MachineAdapters, ProbedAdapter, RunnerCandidate, RunnerName};

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
    /// The runners a person declared, each on its adapter with its model.
    pub declared: &'a BTreeMap<RunnerName, RunnerCandidate>,
    /// The runners proposed and left undeclared, written commented.
    pub undeclared: &'a [RunnerName],
}

impl Page for Initialized<'_> {
    /// The page `init` ends with: what it wrote and detected, as fields; each
    /// adapter this machine answers for, as a check; the lines to paste into
    /// the project's CLAUDE.md; the runners declared; and then a pack to
    /// install, or the runners to declare, because no agent node runs until
    /// its runner is.
    fn document(&self, _look: &Look) -> Doc {
        let adapters = adapters(self.probed);
        let suggestion = claude_md_suggestion();
        let headline = Headline::new(Mark::Done, command::line(&NextStep::Init), "done")
            .fact(format!("in {}", self.repo.display()));
        let doc = Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with(detected_fields(self))
            .with(adapters)
            .with(declared_fields(self.declared))
            .with(Block::Heading(
                "for this repo's CLAUDE.md, to paste yourself: Yunta never writes to that file"
                    .to_string(),
            ))
            .with(Said(suggestion));
        let doctor = (
            command::line(&NextStep::Doctor {
                sessions: false,
                full: false,
            }),
            "confirms everything above is usable",
        );
        if self.undeclared.is_empty() {
            let pack = NextStep::PackAdd { source: Arg::Blank };
            return doc.with(Next {
                steps: vec![
                    (command::line(&pack), "installs a pack of workflows to run"),
                    doctor,
                ],
            });
        }
        doc.with_all(self.to_declare()).with(Next {
            steps: vec![doctor],
        })
    }
}

impl Initialized<'_> {
    /// What the page says of the runners left undeclared, which are written
    /// commented: a probe names an adapter and never a model, and the model is
    /// the project's to choose. With none declared it is the runner to declare,
    /// as `init` has always said it; with some, the ones still to declare.
    fn to_declare(&self) -> Vec<Block> {
        let adapters = MachineAdapters::probed(self.probed);
        let (heading, runners) = match self.declared.is_empty() {
            true => (
                "no runner is declared, so a workflow's agent nodes cannot run until one is"
                    .to_string(),
                declare::runners(self.undeclared, true, &adapters),
            ),
            false => {
                let (are, them, until) = match self.undeclared.len() {
                    1 => ("is", "it", "it is"),
                    _ => ("are", "them", "they are"),
                };
                let undeclared =
                    surco::text::listed(self.undeclared.iter().map(RunnerName::as_str));
                (
                    format!(
                        "{undeclared} {are} not declared, so the agent nodes that name {them} \
                         cannot run until {until}"
                    ),
                    declare::runners(self.undeclared, false, &adapters),
                )
            }
        };
        std::iter::once(Block::Heading(heading))
            .chain(runners.blocks())
            .collect()
    }
}

/// The runners a person declared, each on its adapter with its model, in one
/// field; nothing when none was.
fn declared_fields(declared: &BTreeMap<RunnerName, RunnerCandidate>) -> Fields {
    let said: Vec<String> = declared
        .iter()
        .map(|(runner, candidate)| {
            format!(
                "{runner} on {}, model {}",
                candidate.adapter, candidate.model
            )
        })
        .collect();
    Fields::new().push_if("runners", said.join("; "))
}

/// What `init` wrote and detected, as the fields the page opens with: the
/// files it wrote and the ones it left as they were, then what it found.
fn detected_fields(init: &Initialized<'_>) -> Fields {
    let fields = Fields::new()
        .push_if("wrote", init.wrote.join(", "))
        .push_if("skipped", init.skipped.join(", "));
    found(fields, init.detected)
}

/// Each adapter this machine answers for, as a check: healthy, or unavailable, with
/// what probing it said.
pub(super) fn adapters(probed: &[ProbedAdapter]) -> Checklist {
    let mut adapters = Checklist::default();
    for adapter in probed {
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
    adapters
}

/// `fields` with what a repository was found to have: the ecosystem and its cache
/// tip, and the forge `origin` is on.
pub(super) fn found(fields: Fields, detected: &Detected) -> Fields {
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
    fields
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
