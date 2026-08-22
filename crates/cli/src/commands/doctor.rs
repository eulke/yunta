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

use std::process::ExitCode;

use crate::project;

fn command_on_path(command: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(command).is_file())
}

pub async fn doctor() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let project = match project::resolve(&cwd) {
        Ok(project) => project,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let adapters = super::real_adapters(&project.config);
    let mut all_healthy = true;
    if adapters.is_empty() {
        println!(
            "no adapter to probe — `runners:` in the merged config names none this build \
             supports (only `claude-code` and `codex` are built)"
        );
    } else {
        let mut names: Vec<&String> = adapters.keys().collect();
        names.sort();
        for name in names {
            let adapter = &adapters[name];
            match adapter.probe().await {
                Ok(report) if report.healthy => {
                    println!(
                        "{name}: healthy{}",
                        report
                            .version
                            .as_deref()
                            .map(|v| format!(" ({v})"))
                            .unwrap_or_default()
                    );
                }
                Ok(report) => {
                    all_healthy = false;
                    println!(
                        "{name}: unhealthy — {}",
                        report
                            .diagnostic
                            .as_deref()
                            .unwrap_or("no diagnostic given")
                    );
                }
                Err(e) => {
                    all_healthy = false;
                    println!("{name}: unhealthy — {e}");
                }
            }
        }
    }

    if !check_installed_pack_requires(&cwd, &project.config) {
        all_healthy = false;
    }

    if all_healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
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
        for (_, manifest) in yunta_engine::packs_for_publisher(cwd, &publisher) {
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
            for role in &gap.missing_roles {
                println!(
                    "  role `{role}` — not resolvable: `runners:` doesn't define it, or defines \
                     it with zero candidates; add e.g.:\n      runners:\n        {role}:\n          \
                     - {{ adapter: claude-code, model: <model> }}"
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
