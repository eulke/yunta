//! `yunta pack audit`: prints the full static inventory
//! `yunta_engine::audit_pack` builds for an installed pack — every
//! command, context source, per-node permission, required agent, `mcp`
//! server, executor, and each workflow's full, untrimmed prompt — then
//! reports whether the pack ships tests of its own and whether they
//! pass. Inventory, never verdict: nothing here flags content as
//! suspicious, it only shows all of it. Runs on demand
//! (`yunta pack audit <publisher>/<name>`) and automatically inside
//! `add`, before vendoring — nothing executes until a human has seen
//! the inventory.

use std::path::Path;
use std::process::ExitCode;

use yunta_engine::{audit_pack, NodeAudit, PackAudit, WorkflowAudit};

use super::test::{discover_case_paths, run_case};
use crate::pack::{packs_root, read_manifest, vendor_dir};
use yunta_core::PackRef;

pub async fn audit(pack: &PackRef) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let pack_dir = vendor_dir(&cwd, pack);
    if !pack_dir.is_dir() {
        eprintln!(
            "error: `{pack}` isn't installed under {}",
            packs_root(&cwd).display()
        );
        return ExitCode::FAILURE;
    }
    let manifest = match read_manifest(&pack_dir) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let report = audit_pack(&pack_dir, manifest);
    print_report(&report);
    let tests = run_pack_tests(&pack_dir).await;
    print_test_summary(&tests);
    ExitCode::SUCCESS
}

/// Prints the full inventory — called both by `audit` (on demand) and by
/// `pack add` (automatically, before vendoring).
pub fn print_report(report: &PackAudit) {
    let m = &report.manifest;
    println!("pack: {}/{} @ {}", m.publisher, m.name, m.version);
    println!(
        "declares: permissions={:?} network={} executors={}",
        m.declares.permissions,
        m.declares.network,
        if m.declares.executors.is_empty() {
            "none".to_string()
        } else {
            m.declares.executors.join(", ")
        }
    );
    if !m.requires.runners.is_empty() {
        let runners: Vec<String> = m
            .requires
            .runners
            .iter()
            .map(|r| match &r.permissions {
                Some(p) => format!("{}({:?})", r.name, p),
                None => r.name.to_string(),
            })
            .collect();
        println!("requires runners: {}", runners.join(", "));
    }
    if !m.requires.mcp_servers.is_empty() {
        println!(
            "requires mcp_servers: {}",
            m.requires.mcp_servers.join(", ")
        );
    }
    if !m.requires.commands.is_empty() {
        println!("requires commands: {}", m.requires.commands.join(", "));
    }

    for workflow in &report.workflows {
        print_workflow(workflow);
    }
}

fn print_workflow(workflow: &WorkflowAudit) {
    println!("\nworkflow: {}", workflow.declared_path);
    if let Some(error) = &workflow.error {
        println!("  ERROR: {error}");
        return;
    }
    for node in &workflow.nodes {
        print_node(node);
    }
}

fn print_node(node: &NodeAudit) {
    println!("  node `{}` (kind: {})", node.id, node.kind);
    if let Some(command) = &node.command {
        println!("    command: {command}");
    }
    for step in &node.hooks_before {
        println!("    hook before: {step}");
    }
    for step in &node.hooks_after {
        println!("    hook after: {step}");
    }
    if let Some(permissions) = node.permissions {
        println!("    permissions: {permissions}");
    }
    if let Some(agent) = &node.agent {
        println!("    agent: {agent}");
    }
    for server in &node.mcp_servers {
        println!("    mcp_server: {server}");
    }
    if let Some(executor) = &node.executor {
        println!("    executor (code): {executor}");
    }
    for entry in &node.context {
        println!("    context: {entry}");
    }
    match &node.prompt {
        None => {}
        Some(Ok(text)) => {
            println!("    prompt:");
            for line in text.lines() {
                println!("      {line}");
            }
        }
        Some(Err(error)) => println!("    prompt: UNREADABLE — {error}"),
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

pub async fn run_pack_tests(pack_dir: &Path) -> PackTestSummary {
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
        match run_case(pack_dir, case_path).await {
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
            "\ntests: {} case(s) shipped, not run (pass --run-tests)",
            summary.total
        );
    } else {
        println!(
            "\ntests: {} case(s), {} failed",
            summary.total, summary.failed
        );
        for line in &summary.failures {
            println!("  {line}");
        }
    }
}
