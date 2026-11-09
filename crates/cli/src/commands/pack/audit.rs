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

use surco::look::Look;
use yunta_core::PackRef;
use yunta_engine::audit_pack;
use yunta_render::page::pack::{Cases, Inventory};
use yunta_render::page::RanCase;

use crate::commands::test::{case_name, case_run, discover_case_paths, run_case};
use crate::error::CliError;
use crate::output::show;
use crate::output::Outcome;
use crate::pack::{packs_root, read_manifest, vendor_dir};
use crate::shell::Shell;

pub async fn audit(shell: &Shell, pack: &PackRef, look: &Look) -> Result<Outcome, CliError> {
    let ctx = crate::context::Context::of(shell)?;
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
    let cases = run_cases(shell, &pack_dir).await;
    let inventory = Inventory::Audited {
        audit: &report,
        pack,
        cases: &cases,
    };
    show(look, &inventory);
    Ok(Outcome::Success)
}

/// The pack's cases as a count only — what `add` reports when nothing
/// of the pack is to run.
pub fn count_cases(pack_dir: &Path) -> Cases {
    match discover_case_paths(pack_dir).map_or(0, |paths| paths.len()) {
        0 => Cases::NoneShipped,
        shipped => Cases::NotRun { shipped },
    }
}

/// The pack's cases, each run on its vendored copy the way `yunta test`
/// runs a project's own: none shipped where it has no `.yunta/tests/`, or
/// an empty one.
pub async fn run_cases(shell: &Shell, pack_dir: &Path) -> Cases {
    let case_paths = discover_case_paths(pack_dir).unwrap_or_default();
    if case_paths.is_empty() {
        return Cases::NoneShipped;
    }
    let mut cases = Vec::new();
    for case_path in &case_paths {
        cases.push(RanCase {
            name: case_name(case_path),
            run: case_run(run_case(shell, pack_dir, case_path).await),
        });
    }
    Cases::Ran(cases)
}
