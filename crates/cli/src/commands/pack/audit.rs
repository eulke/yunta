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

use surco::components::{Check, Checklist, Fields, Found};
use surco::doc::Block;
use surco::glyphs::Mark;
use surco::look::Look;
use surco::text::counted;
use yunta_core::PackRef;
use yunta_engine::audit_pack;

use super::page::Inventory;
use crate::commands::test::{case_found, discover_case_paths, run_case};
use crate::error::CliError;
use crate::interrupt::Interrupt;
use crate::output::show;
use crate::output::Outcome;
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
    let inventory = Inventory::Audited {
        audit: &report,
        pack,
        tests: &tests,
    };
    show(look, &inventory);
    Ok(Outcome::Success)
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
