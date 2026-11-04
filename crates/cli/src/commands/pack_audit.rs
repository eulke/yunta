//! `yunta pack audit`: prints the full static inventory
//! `yunta_engine::audit_pack` builds for an installed pack — every
//! command, context source, per-node permission, required agent, `mcp`
//! server, executor, repository path a scope names, and each workflow's
//! full, untrimmed prompt — then
//! reports whether the pack ships tests of its own and whether they
//! pass. Inventory, never verdict: nothing here flags content as
//! suspicious, it only shows all of it. Runs on demand
//! (`yunta pack audit <publisher>/<name>`) and automatically inside
//! `add`, before vendoring — nothing executes until a human has seen
//! the inventory.

use std::path::Path;

use surco::components::{Check, Checklist, Code, Fields, Found, Furrow, Headline, Marked, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::text::counted;
use yunta_core::PackRef;
use yunta_engine::{audit_pack, NodeAudit, PackAudit, WorkflowAudit, WorkflowUnread};

use super::test::{case_found, discover_case_paths, run_case};
use crate::error::{page, CliError, Outcome};
use crate::interrupt::Interrupt;
use crate::output::document;
use crate::pack::{packs_root, read_manifest, vendor_dir};

pub async fn audit(pack: &PackRef, look: &Look) -> Result<Outcome, CliError> {
    let ctx = crate::context::Context::load()?;
    let cwd = ctx.cwd.clone();
    let pack_dir = vendor_dir(&cwd, pack);
    if !pack_dir.is_dir() {
        return Err(CliError::msg(format!(
            "`{pack}` isn't installed under {}",
            packs_root(&cwd).display()
        )));
    }
    let manifest = read_manifest(&pack_dir)?;

    let report = audit_pack(&pack_dir, manifest);
    let tests = run_pack_tests(&pack_dir, ctx.interrupt()).await;
    let headline = Headline::new(tests.mark(), format!("pack {pack}"), "audited")
        .fact(report.manifest.version.to_string());
    document(look, inventory(&report, headline).with_all(tests.blocks()));
    Ok(Outcome::Success)
}

/// The inventory as a page under `headline`: what the pack declares, then
/// each workflow, node by node. `audit` shows it on demand, and `pack add`
/// before anything is vendored.
pub fn inventory(report: &PackAudit, headline: Headline) -> Doc {
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
        Some(WorkflowUnread::Refused { report, text }) => page::hinted(
            report
                .diagnostics
                .iter()
                .map(|diagnostic| page::found(diagnostic, &workflow.declared_path, Some(text)))
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

/// Whether the pack ships tests under its own `.yunta/tests/` (same case
/// format `yunta test` uses, authored the same way a repo's own tests
/// are) and, if so, how many pass. `has_tests: false` covers both
/// "no `.yunta/tests/` directory" and "directory present but empty";
/// either way there's nothing to report a pass/fail count for.
pub struct PackTestSummary {
    pub has_tests: bool,
    pub total: usize,
    pub failed: usize,
    /// Each case that failed: its name, what its row says, and every
    /// problem it had, as written.
    pub failures: Vec<(String, String, Vec<String>)>,
    /// Whether the cases ran: `add` counts them without running them
    /// unless asked, and says so.
    pub ran: bool,
}

/// The pack's cases as a count only — what `add` reports when nothing
/// of the pack is to run.
pub fn count_pack_tests(pack_dir: &Path) -> PackTestSummary {
    let total = discover_case_paths(pack_dir).map_or(0, |paths| paths.len());
    PackTestSummary {
        has_tests: total > 0,
        total,
        failed: 0,
        failures: Vec::new(),
        ran: false,
    }
}

pub async fn run_pack_tests(pack_dir: &Path, interrupt: Interrupt) -> PackTestSummary {
    let empty = PackTestSummary {
        has_tests: false,
        total: 0,
        failed: 0,
        failures: Vec::new(),
        ran: true,
    };
    let Some(case_paths) = discover_case_paths(pack_dir) else {
        return empty;
    };
    if case_paths.is_empty() {
        return empty;
    }

    let mut failed = 0;
    let mut failures = Vec::new();
    for case_path in &case_paths {
        let name = case_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| case_path.display().to_string());
        let (found, said, problems) =
            case_found(run_case(pack_dir, case_path, interrupt.shared()).await);
        if found != Found::Holds {
            failed += 1;
            failures.push((name, said, problems));
        }
    }
    PackTestSummary {
        has_tests: true,
        total: case_paths.len(),
        failed,
        failures,
        ran: true,
    }
}

impl PackTestSummary {
    /// What the pack's cases say, on the row they get: none shipped, how
    /// many were shipped and not run, or how many ran and how many failed.
    pub fn said(&self) -> String {
        match (self.has_tests, self.ran) {
            (false, _) => "none shipped".to_string(),
            (true, false) => format!(
                "{} shipped, not run (pass --run-tests)",
                counted(self.total, "case")
            ),
            (true, true) => format!("{}, {} failed", counted(self.total, "case"), self.failed),
        }
    }

    /// The mark a page about the pack opens with: what the page did held,
    /// and a failed case is worth a look.
    pub fn mark(&self) -> Mark {
        match self.failed {
            0 => Mark::Done,
            _ => Mark::Caution,
        }
    }

    /// The cases' row, and a row for each case that failed with every
    /// problem it had under it, each line as written, as `yunta test` lists
    /// them.
    pub fn blocks(&self) -> Vec<Block> {
        let mut failed = Checklist::default();
        for (case, said, problems) in &self.failures {
            failed.push_check(
                Check::new(Found::Problem, case.as_str(), said.as_str()).details(problems.clone()),
            );
        }
        let failed = (!self.failures.is_empty()).then(|| failed.into());
        std::iter::once(Fields::new().push_if("tests", self.said()).into())
            .chain(failed)
            .collect()
    }
}
