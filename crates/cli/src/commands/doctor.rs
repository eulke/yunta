//! `yunta doctor`: probes every adapter this project's `runners:`
//! names — binary present, version compatible, auth valid — and
//! reports each one, healthy or not. The same `probe()` a real
//! `yunta run`/`yunta resume` calls before spending anything (see
//! `commands::probe_or_refuse`); this command exists to run it on
//! demand and report every result instead of stopping at the first
//! failure.
//!
//! Also validates every installed pack's own `requires:` against this
//! project's merged config: roles resolvable, `mcp_servers:` defined,
//! and — the one part `yunta_engine::check_pack_requires` deliberately
//! leaves to this command, since it needs real filesystem access —
//! `requires.commands` present on `PATH`.

use crate::context::Context;
use crate::error::{CliError, Outcome};
use yunta_adapters::ProbeReport;
use yunta_core::AdapterId;

fn command_on_path(command: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(command).is_file())
}

pub async fn doctor() -> Result<Outcome, CliError> {
    let ctx = Context::load()?;

    let adapters = ctx.adapters();
    let mut all_healthy = true;
    if adapters.is_empty() {
        println!(
            "no adapter to probe — `runners:` in the merged config names none this build \
             supports (only `claude-code` and `codex` are built)"
        );
    } else {
        let mut names: Vec<&AdapterId> = adapters.keys().collect();
        names.sort();
        for name in names {
            let Some(adapter) = adapters.get(name) else {
                continue;
            };
            match adapter.probe().await {
                Ok(ProbeReport::Healthy { version }) => {
                    println!(
                        "{name}: healthy{}",
                        version
                            .as_deref()
                            .map(|v| format!(" ({v})"))
                            .unwrap_or_default()
                    );
                }
                Ok(ProbeReport::Unhealthy { diagnostic }) => {
                    all_healthy = false;
                    println!("{name}: unhealthy — {diagnostic}");
                }
                Err(e) => {
                    all_healthy = false;
                    println!("{name}: unhealthy — {e}");
                }
            }
        }
    }

    if !check_installed_pack_requires(&ctx.cwd, &ctx.project.config) {
        all_healthy = false;
    }

    if all_healthy {
        Ok(Outcome::Success)
    } else {
        Ok(Outcome::Reported)
    }
}

/// Checks every installed pack's own `requires:` against this
/// project's merged config — roles resolvable, `mcp_servers:` defined,
/// and `requires.commands` present on `PATH`. Returns `false` (and
/// prints an actionable line per gap) when any pack has something
/// unmet; a project with no packs installed prints nothing and returns
/// `true`.
fn check_installed_pack_requires(cwd: &std::path::Path, config: &yunta_core::ConfigLayer) -> bool {
    let mut all_satisfied = true;
    for publisher in yunta_engine::installed_publishers(cwd) {
        let packs = yunta_engine::packs_for_publisher(cwd, &publisher);
        // A broken pack is a real gap: its manifest is the only place its
        // requirements are declared, so it is named, never skipped.
        for err in &packs.broken {
            all_satisfied = false;
            println!("{err}");
        }
        for (_, manifest) in packs.installed {
            let gap = yunta_engine::check_pack_requires(&manifest, config);
            let missing_commands: Vec<&String> = gap
                .required_commands
                .iter()
                .filter(|cmd| !command_on_path(cmd))
                .collect();
            if gap.is_satisfied() && missing_commands.is_empty() {
                continue;
            }
            all_satisfied = false;
            println!("pack {} requires:", gap.pack);
            for runner in &gap.missing_runners {
                println!(
                    "  runner `{runner}` — not resolvable: `runners:` doesn't define it, or \
                     defines it with zero candidates; add e.g.:\n      runners:\n        \
                     {runner}:\n          - {{ adapter: claude-code, model: <model> }}"
                );
            }
            for server in &gap.missing_mcp_servers {
                println!(
                    "  mcp_server `{server}` — not defined under `mcp_servers:`; add it there"
                );
            }
            for command in missing_commands {
                println!("  command `{command}` — not found on PATH");
            }
        }
    }
    all_satisfied
}
