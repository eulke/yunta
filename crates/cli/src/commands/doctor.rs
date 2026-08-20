//! `yunta doctor` (T7.1, Spec Adapter §2): probes every adapter this
//! project's `runners:` names — binary present, version compatible, auth
//! valid — and reports each one, healthy or not. The same `probe()` a
//! real `yunta run`/`yunta resume` calls before spending anything (see
//! `commands::probe_or_refuse`); this command exists to run it on
//! demand and report every result instead of stopping at the first
//! failure.

use std::process::ExitCode;

use crate::project;

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
    if adapters.is_empty() {
        println!(
            "no adapter to probe — `runners:` in the merged config names none this build \
             supports (only `claude-code` is built, T7.4 adds more)"
        );
        return ExitCode::SUCCESS;
    }

    let mut names: Vec<&String> = adapters.keys().collect();
    names.sort();

    let mut all_healthy = true;
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

    if all_healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
