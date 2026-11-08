//! What a pack command says, each a page filled with what the command read:
//! the inventory a person reads before a pack is vendored or updated, or of
//! an installed pack audited; what an install, an update, a removal and a
//! listing answer with; and why `add` refuses a pack that is installed
//! already.

use std::path::Path;

use surco::components::{Checklist, Code, Fields, Found, Furrow, Headline, Marked, Said, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::text::counted;
use surco::words::Standing;
use yunta_core::step::{Arg, NextStep};
use yunta_core::{CommitSha, PackManifest, PackRef};
use yunta_engine::{NodeAudit, PackAudit, WorkflowAudit, WorkflowUnread};
use yunta_render::command;
use yunta_render::page::Page;

use super::audit::PackTestSummary;
use super::needs::Need;

/// What an install answers with: the pack, its version and commit, where it
/// is, what its own cases say, and what it needs from this project.
pub(super) struct Installed<'a> {
    pub(super) manifest: &'a PackManifest,
    pub(super) commit: &'a CommitSha,
    pub(super) at: &'a Path,
    pub(super) cwd: &'a Path,
    pub(super) tests: &'a PackTestSummary,
    /// What its workflows need from this project and it does not declare.
    pub(super) needs: Vec<Need>,
}

impl Installed<'_> {}

impl Page for Installed<'_> {
    /// The page: installed, unless one of the pack's own cases failed,
    /// which is worth a look.
    fn document(&self, _look: &Look) -> Doc {
        let pack = format!("pack {}", self.manifest.reference());
        let headline = Headline::new(self.tests.mark(), pack, "installed")
            .fact(self.manifest.version.to_string())
            .fact(self.commit.abbreviated());
        let at = surco::paths::shown(self.at, self.cwd, None);
        Doc::new()
            .with(headline)
            .with(Furrow(Vec::new()))
            .with(Fields::new().push_if("at", at))
            .with_all(self.tests.blocks())
            .with_all(needs(&self.needs))
    }
}

/// Why `add` refuses `pack`, installed already at `dest`: the lines that
/// change its ref, or take it out first.
pub(super) fn already_installed(pack: &PackRef, dest: &Path) -> String {
    let update = NextStep::PackUpdate {
        pack: pack.clone(),
        reference: Arg::Blank,
    };
    let remove = NextStep::PackRemove { pack: pack.clone() };
    format!(
        "`{pack}` is already installed at {}; use `{}` to change its ref, or `{}` first",
        dest.display(),
        command::line(&update),
        command::line(&remove)
    )
}

/// Why `update` refuses `pack`, which is not installed: the line that adds it.
pub(super) fn not_installed(pack: &PackRef) -> String {
    let add = NextStep::PackAdd { source: Arg::Blank };
    format!(
        "`{pack}` isn't installed; `{}` it first, not update",
        command::line(&add)
    )
}

/// A pack moved to a new ref, as the person who asked for it reads it: the
/// ref it is at now, and the commit that ref resolved to.
pub(super) struct Updated<'a> {
    pub(super) pack: &'a PackRef,
    pub(super) to: &'a str,
    pub(super) commit: &'a CommitSha,
}

impl Page for Updated<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let headline = Headline::new(Mark::Done, format!("pack {}", self.pack), "updated")
            .fact(format!("to {}", self.to))
            .fact(self.commit.abbreviated());
        Doc::new().with(headline)
    }
}

/// A pack taken out of the project, as the person who asked for it reads it.
pub(super) struct Removed<'a> {
    pub(super) pack: &'a PackRef,
}

impl Page for Removed<'_> {
    fn document(&self, _look: &Look) -> Doc {
        Doc::new().with(Headline::new(
            Mark::Done,
            format!("pack {}", self.pack),
            "removed",
        ))
    }
}

/// The packs a project has installed, as `pack list` says them.
pub(super) enum Listed {
    /// No pack is installed under `looked_in`.
    Empty { looked_in: String },
    /// Each pack the lock holds, as its vendored tree stands against the
    /// lock, and where the lock is.
    Locked {
        packs: Vec<VendoredPack>,
        lock: String,
    },
}

/// One installed pack, as its vendored tree stands against the lock.
pub(super) struct VendoredPack {
    pub(super) pack: String,
    pub(super) found: Found,
    pub(super) said: String,
}

impl Page for Listed {
    fn document(&self, _look: &Look) -> Doc {
        match self {
            Listed::Empty { looked_in } => Doc::new()
                .with(Headline::new(Mark::Absent, "packs", "none installed"))
                .with(Furrow(Vec::new()))
                .with(Fields::new().push_if("looked in", looked_in.as_str())),
            Listed::Locked { packs, lock } => {
                let mut checks = Checklist::default();
                for vendored in packs {
                    checks.push(
                        vendored.found,
                        vendored.pack.as_str(),
                        vendored.said.as_str(),
                    );
                }
                let mark = match checks.holds() {
                    true => Mark::Done,
                    false => Mark::Failed,
                };
                let installed = Headline::new(mark, "packs", format!("{} installed", packs.len()))
                    .tally(checks.count(Found::Problem), checks.count(Found::Caution));
                Doc::new()
                    .with(installed)
                    .with(Furrow(Vec::new()))
                    .with(checks)
                    .with(Fields::new().push_if("lock", lock.as_str()))
            }
        }
    }
}

/// What a pack's workflows need from this project, under a heading of its
/// own: a line for each, saying whether a run is refused for it or leaves
/// its node out, and under it what this repository answers for it. Each
/// line once, so what two needs share is said under the first. Nothing when
/// they need nothing.
fn needs(needs: &[Need]) -> Vec<Block> {
    let mut lines: Vec<String> = Vec::new();
    for need in needs {
        let fate = match need.leaves_node_out {
            true => "a run leaves the node out until it does",
            false => "a run is refused until it does",
        };
        let said = format!(
            "node `{}`: the project declares {}; {fate}",
            need.node,
            need.key.undeclared()
        );
        let detected = need
            .detected
            .as_ref()
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

/// A pack's inventory, as a person reads it: what the pack declares, then
/// each workflow, node by node, under what heads it.
pub(super) enum Inventory<'a> {
    /// What a person reads before the pack is installed: it asks for them
    /// when the executors the pack declares wait on their `--yes`.
    Installing {
        audit: &'a PackAudit,
        waits_on_yes: bool,
    },
    /// What a person reads before the pack is updated in place, the same way.
    Updating {
        audit: &'a PackAudit,
        waits_on_yes: bool,
    },
    /// An installed pack, audited: its inventory, then what its own cases say.
    Audited {
        audit: &'a PackAudit,
        pack: &'a PackRef,
        tests: &'a PackTestSummary,
    },
}

impl Page for Inventory<'_> {
    fn document(&self, _look: &Look) -> Doc {
        match *self {
            Inventory::Installing {
                audit,
                waits_on_yes,
            } => inventory(audit, before(audit, waits_on_yes, "installing")),
            Inventory::Updating {
                audit,
                waits_on_yes,
            } => inventory(audit, before(audit, waits_on_yes, "updating")),
            Inventory::Audited { audit, pack, tests } => {
                let headline = Headline::new(tests.mark(), format!("pack {pack}"), "audited")
                    .fact(audit.manifest.version.to_string());
                inventory(audit, headline).with_all(tests.blocks())
            }
        }
    }
}

/// The headline of the inventory a person reads before the pack is vendored,
/// `doing` it: it asks for them when the executors the pack declares wait on
/// their `--yes`.
fn before(audit: &PackAudit, waits_on_yes: bool, doing: &str) -> Headline {
    let manifest = &audit.manifest;
    let pack = format!("pack {}", manifest.reference());
    let headline = match waits_on_yes {
        true => Headline::new(Mark::NeedsYou, pack, Standing::NeedsYou.to_string()),
        false => Headline::new(Mark::Running, pack, doing),
    }
    .fact(manifest.version.to_string());
    match manifest.declares.executors.len() {
        0 => headline,
        executors => headline.fact(counted(executors, "executor")),
    }
}

/// The inventory under `headline`: what the pack declares, then each
/// workflow, node by node.
fn inventory(report: &PackAudit, headline: Headline) -> Doc {
    let m = &report.manifest;
    let runners: Vec<String> = m
        .requires
        .runners
        .iter()
        .map(|r| r.name.to_string())
        .collect();
    let declares = Fields::new()
        .push_if("permissions", m.declares.permissions.as_str())
        .push_if(
            "network",
            match m.declares.network {
                true => "used",
                false => "none",
            },
        )
        .push_if(
            "executors",
            match m.declares.executors.is_empty() {
                true => "none".to_string(),
                false => m.declares.executors.join(", "),
            },
        )
        .push_if("runners", runners.join(", "))
        .push_if("mcp servers", m.requires.mcp_servers.join(", "))
        .push_if("programs", m.requires.programs.join(", "));
    let mut doc = Doc::new()
        .with(headline)
        .with(Furrow(Vec::new()))
        .with(declares);
    for workflow in &report.workflows {
        doc = doc.with(workflow_section(workflow));
    }
    doc
}

fn workflow_section(workflow: &WorkflowAudit) -> Section {
    let blocks = match &workflow.error {
        Some(WorkflowUnread::Missing(error)) => vec![Marked {
            mark: Mark::Failed,
            items: vec![format!("does not read: {error}")],
        }
        .into()],
        Some(WorkflowUnread::Refused { report, text }) => crate::error::page::hinted(
            report
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    crate::error::page::found(diagnostic, &workflow.declared_path, Some(text))
                })
                .collect(),
        )
        .into_iter()
        .map(Block::from)
        .collect(),
        None => workflow
            .nodes
            .iter()
            .map(|node| node_section(node).into())
            .collect(),
    };
    Section {
        lead: None,
        title: Line::new()
            .plain("workflow ")
            .push(Tone::Strong, workflow.declared_path.to_string()),
        blocks,
    }
}

/// What a node runs, may do and is given, one row each.
fn node_fields(node: &NodeAudit) -> Fields {
    let mut fields = Fields::new();
    if let Some(command) = &node.command {
        fields = fields.push_command("command", command.as_str());
    }
    for step in &node.hooks_before {
        fields = fields.push_command("hook before", step.to_string());
    }
    for step in &node.hooks_after {
        fields = fields.push_command("hook after", step.to_string());
    }
    if let Some(permissions) = node.permissions {
        fields = fields.push_if("permissions", permissions.to_string());
    }
    if let Some(agent) = &node.agent {
        fields = fields.push_if("agent", agent.to_string());
    }
    for server in &node.mcp_servers {
        fields = fields.push_if("mcp server", server.to_string());
    }
    if let Some(executor) = &node.executor {
        fields = fields.push_if("executor", format!("{executor}, code that runs"));
    }
    fields = fields.push_if("paths", node.paths.join(", "));
    for entry in &node.context {
        fields = fields.push_if("context", entry.to_string());
    }
    fields
}

fn node_section(node: &NodeAudit) -> Section {
    let fields = node_fields(node);
    let mut blocks: Vec<Block> = vec![fields.into()];
    match &node.prompt {
        None => {}
        Some(Ok(text)) => blocks.push(Code::whole("prompt", None, text).into()),
        Some(Err(error)) => blocks.push(
            Marked {
                mark: Mark::Failed,
                items: vec![format!(
                    "prompt unreadable: {}",
                    yunta_core::describe(error)
                )],
            }
            .into(),
        ),
    }
    Section {
        lead: None,
        title: Line::new()
            .plain("node ")
            .push(Tone::Strong, format!("`{}`", node.id))
            .push(Tone::Muted, format!(" {}", node.kind)),
        blocks,
    }
}
