//! A pack's inventory, as a person reads it before the pack is vendored or
//! updated, or of an installed pack audited: what the pack declares, then
//! each workflow, node by node — every command, context source,
//! permission, agent, server, executor and repository path, and its full
//! prompt. Inventory, never verdict: it shows everything and flags nothing.

use surco::components::{Code, Fields, Furrow, Headline, Marked, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::text::counted;
use surco::words::Standing;
use yunta_core::{NodeAudit, PackAudit, PackRef, WorkflowAudit, WorkflowUnread};

use super::Cases;
use crate::blocks::diagnostic::{hinted, Pointed};
use crate::page::Page;

/// A pack's inventory, as a person reads it: what the pack declares, then
/// each workflow, node by node, under what heads it.
pub enum Inventory<'a> {
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
        cases: &'a Cases,
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
            Inventory::Audited { audit, pack, cases } => {
                let headline = Headline::new(cases.mark(), format!("pack {pack}"), "audited")
                    .fact(audit.manifest.version.to_string());
                inventory(audit, headline).with_all(cases.blocks())
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
        Some(WorkflowUnread::Unreadable(error)) => vec![Marked {
            mark: Mark::Failed,
            items: vec![format!("does not read: {}", yunta_core::describe(error))],
        }
        .into()],
        Some(WorkflowUnread::Refused { report, text }) => hinted(
            report
                .diagnostics
                .iter()
                .map(|diagnostic| Pointed::found(diagnostic, &workflow.declared_path, Some(text)))
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
