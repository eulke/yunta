//! What `yunta init` did and found, as a person reads it: the files it wrote
//! and the ones it left, the language and the forge it detected, in the words
//! a setup says them ([`Setup`]), each adapter this machine answers for, the
//! lines for the project's CLAUDE.md, and the runners it wrote ([`WrittenRunners`]):
//! every one declared, each with its candidates in order and the default runner
//! said, then a pack of workflows to install; or, where no adapter answered,
//! every one proposed, with the runners to declare, since no agent node runs
//! until its runner is.

use std::path::Path;

use surco::components::{Checklist, Fields, Found, Furrow, Headline, Said};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::{Arg, NextStep};
use yunta_core::{Detected, MachineAdapters, ProbedAdapter, RunnerCandidate, RunnerName};

use super::setup::from_origin;
use super::{Page, Setup};
use crate::command::{self, SaidStep};
use crate::declare;

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
    /// The runners it wrote.
    pub runners: WrittenRunners<'a>,
    /// The runner a node that names none runs on, `defaults.runner`, where there is one.
    pub default: Option<&'a RunnerName>,
}

/// The runners `init` wrote: every role declared, or every role proposed where no
/// adapter answered on the machine. Never some of each: a role is left undeclared
/// only where none could be declared.
pub enum WrittenRunners<'a> {
    /// Each role, in the order written, on its candidates in the order a run tries them.
    Declared(Vec<(&'a RunnerName, &'a [RunnerCandidate])>),
    /// Each role proposed, written commented.
    Proposed(&'a [RunnerName]),
}

impl Page for Initialized<'_> {
    /// The page `init` ends with: what it wrote and detected, as fields; each
    /// adapter this machine answers for, as a check; the runners declared, with the
    /// default said; the lines to paste into the project's CLAUDE.md; and then a
    /// pack to install, or the runners to declare, because no agent node runs
    /// until its runner is.
    fn document(&self, _look: &Look) -> Doc {
        let adapters = adapters(self.probed);
        let suggestion = claude_md_suggestion();
        let headline = Headline::new(
            Mark::Done,
            command::line(&NextStep::Init { interactive: false }),
            "done",
        )
        .fact(format!("in {}", self.repo.display()));
        let runners = match &self.runners {
            WrittenRunners::Declared(declared) => declared_fields(declared, self.default),
            WrittenRunners::Proposed(_) => Fields::new(),
        };
        let doc = Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with(detected_fields(self))
            .with(adapters)
            .with(runners)
            .with(Block::Heading(
                "for this repo's CLAUDE.md, to paste yourself: Yunta never writes to that file"
                    .to_string(),
            ))
            .with(Said(suggestion));
        let doctor = SaidStep::of(&NextStep::Doctor {
            sessions: false,
            full: false,
        });
        match &self.runners {
            WrittenRunners::Declared(_) => {
                let pack = SaidStep::of(&NextStep::PackAdd { source: Arg::Blank });
                doc.with(command::next([pack, doctor]))
            }
            WrittenRunners::Proposed(roles) => doc
                .with_all(self.to_declare(roles))
                .with(command::next([doctor])),
        }
    }
}

impl Initialized<'_> {
    /// What the page says of the runners `roles` proposed, written commented where no
    /// adapter answered on the machine: that none is declared, and the runners to
    /// declare, with the default runner where there is one.
    fn to_declare(&self, roles: &[RunnerName]) -> Vec<Block> {
        let adapters = MachineAdapters::probed(self.probed);
        let heading = "no runner is declared, so a workflow's agent nodes cannot run until one is";
        let runners = declare::runners(roles, self.default.is_some(), &adapters);
        std::iter::once(Block::Heading(heading.to_string()))
            .chain(runners.blocks())
            .collect()
    }
}

/// The runners `declared`, in one field, in the order written, each on its first
/// candidate, then its others in order, each candidate as a setup's row says it
/// ([`crate::runner`]), the `default` runner said as it (`implementer (the default) on
/// claude-code/opus, then codex`).
fn declared_fields(
    declared: &[(&RunnerName, &[RunnerCandidate])],
    default: Option<&RunnerName>,
) -> Fields {
    let said: Vec<String> = declared
        .iter()
        .filter_map(|(runner, candidates)| {
            let (first, others) = candidates.split_first()?;
            let model = first.model.as_ref();
            let on = match default == Some(*runner) {
                true => {
                    crate::runner::on(crate::runner::the_default(runner), &first.adapter, model)
                }
                false => crate::runner::on(runner, &first.adapter, model),
            };
            let others: Vec<String> = others
                .iter()
                .map(|other| crate::runner::candidate(&other.adapter, other.model.as_ref()))
                .collect();
            Some(match others.is_empty() {
                true => on,
                false => format!("{on}, {} {}", Setup::THEN, others.join(", ")),
            })
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
fn adapters(probed: &[ProbedAdapter]) -> Checklist {
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

/// `fields` with what a repository was found to have: its language, with what was written
/// for it, and its cache tip; and the forge `origin` is on, in the words a setup says them,
/// with the variable the forge's token is read from.
fn found(fields: Fields, detected: &Detected) -> Fields {
    let ecosystem = match &detected.ecosystem {
        Some(ecosystem) => format!(
            "{}: wrote {}{}",
            ecosystem.language,
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
        .push_if(Setup::LANGUAGE, ecosystem)
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
                    format!(
                        "{}; its token is read from `GITHUB_TOKEN`",
                        from_origin(repo)
                    )
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
