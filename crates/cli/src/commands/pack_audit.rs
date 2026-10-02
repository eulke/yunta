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

use yunta_engine::{audit_pack, NodeAudit, PackAudit, WorkflowAudit};

use super::test::{discover_case_paths, run_case};
use crate::error::{CliError, Outcome};
use crate::pack::{packs_root, read_manifest, vendor_dir};
use crate::render::blocks::{Code, Fields, Marked, Section};
use crate::render::doc::{Block, Doc};
use crate::render::ink::{Line, Tone};
use crate::render::Mark;
use yunta_core::PackRef;

pub async fn audit(pack: &PackRef) -> Result<Outcome, CliError> {
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
    print_report(&report);
    let tests = run_pack_tests(&pack_dir, &ctx).await;
    print_test_summary(&tests);
    Ok(Outcome::Success)
}

/// Prints the full inventory — called both by `audit` (on demand) and by
/// `pack add` (automatically, before vendoring).
pub fn print_report(report: &PackAudit) {
    let look = crate::render::stdout_look();
    print!("{}", crate::render::draw(inventory(report), &look));
}

/// The inventory as a document: what the pack is and declares, then each
/// workflow, node by node.
fn inventory(report: &PackAudit) -> Doc<'static> {
    let m = &report.manifest;
    let runners: Vec<String> = m
        .requires
        .runners
        .iter()
        .map(|r| match &r.permissions {
            Some(p) => format!("{} ({})", r.name, p.as_str()),
            None => r.name.to_string(),
        })
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
        .with(Block::Title(Line::new().push(
            Tone::Strong,
            format!("pack {}/{} @ {}", m.publisher, m.name, m.version),
        )))
        .with(declares);
    for workflow in &report.workflows {
        doc = doc.with(workflow_section(workflow));
    }
    doc
}

fn workflow_section(workflow: &WorkflowAudit) -> Section<'static> {
    let blocks = match &workflow.error {
        Some(error) => vec![Marked {
            mark: Mark::Failed,
            items: vec![format!("does not read: {error}")],
        }
        .into()],
        None => workflow
            .nodes
            .iter()
            .map(|node| node_section(node).into())
            .collect(),
    };
    Section {
        mark: None,
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

fn node_section(node: &NodeAudit) -> Section<'static> {
    let fields = node_fields(node);
    let mut blocks: Vec<Block<'static>> = vec![fields.into()];
    match &node.prompt {
        None => {}
        Some(Ok(text)) => blocks.push(
            Code {
                at: "prompt".to_string(),
                what: None,
                lines: text.trim_end().lines().map(str::to_string).collect(),
                whole: text.trim_end().lines().count(),
                rest: None,
            }
            .into(),
        ),
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
        mark: None,
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
    pub failures: Vec<String>,
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

pub async fn run_pack_tests(pack_dir: &Path, ctx: &crate::context::Context) -> PackTestSummary {
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
        match run_case(pack_dir, case_path, ctx.interrupt()).await {
            Ok(problems) if problems.is_empty() => {}
            Ok(problems) => {
                failed += 1;
                for problem in problems {
                    failures.push(format!("{name}: {problem}"));
                }
            }
            Err(error) => {
                failed += 1;
                failures.push(format!("{name}: {error}"));
            }
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

pub fn print_test_summary(summary: &PackTestSummary) {
    if !summary.has_tests {
        println!("\ntests: none shipped");
    } else if !summary.ran {
        println!(
            "\ntests: {} shipped, not run (pass --run-tests)",
            yunta_core::text::counted(summary.total, "case")
        );
    } else {
        // The heading counts cases and the lines under it count
        // problems — one failing case contributes as many as it has —
        // so each count stays with what it counts, and the block is
        // indented rather than given a second, different total.
        println!(
            "\ntests: {}, {} failed",
            yunta_core::text::counted(summary.total, "case"),
            summary.failed
        );
        if !summary.failures.is_empty() {
            println!(
                "{}",
                yunta_core::text::indent(&summary.failures.join("\n"), "  ")
            );
        }
    }
}
