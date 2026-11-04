//! The permission profile a Codex session runs under: the `-c` overrides
//! that name it, and the lowest CLI version that honors them.

use std::path::Path;

use yunta_core::port::ProbeReport;
use yunta_core::{Commands, Confinement, Unbuildable};

use super::config::ConfigOverride;

/// The name of the permission profile every confined session selects.
pub(super) const PROFILE: &str = "yunta-session";
/// The lowest Codex CLI version measured to honor the profile keys;
/// `probe()` reads the last token of `codex-cli <v>`.
pub(super) const VERSION_FLOOR: (u64, u64, u64) = (0, 157, 0);

/// The arguments that put one session under `conf`.
pub(super) fn overrides(conf: &Confinement, cwd: &Path) -> Result<Vec<String>, Unbuildable> {
    if conf.commands == Commands::Unconfined {
        // The unconfined mode names no profile: the CLI's own mode is the whole of it.
        if !conf.network {
            return Err(Unbuildable::NetworkUnclosable);
        }
        return Ok(vec!["--sandbox".into(), "danger-full-access".into()]);
    }
    let fs = filesystem(conf, cwd);
    let key = |name: &str| format!("permissions.{PROFILE}.{name}");
    // `extends` is always set: without it the CLI aborts on :project_roots.
    let base = if conf.writes_worktree() {
        ":workspace"
    } else {
        ":read-only"
    };
    let mut args = ConfigOverride::string("default_permissions", PROFILE)
        .into_args()
        .to_vec();
    args.extend(ConfigOverride::string(key("extends"), base).into_args());
    args.extend(ConfigOverride::value(key("filesystem"), toml::Value::Table(fs)).into_args());
    args.extend(ConfigOverride::bool(key("network.enabled"), conf.network).into_args());
    Ok(args)
}

/// The directory Codex holds read-only under a writable root unless a
/// profile names it, though of it Codex loads only its skills root.
const AGENTS_DIR: &str = ".agents";

/// The profile's filesystem table: temp and the session's roots writable,
/// the checkout as the session may write it, and every protected tree
/// read-only. A session that writes the checkout has `AGENTS_DIR` named
/// writable too, so the project's files there are the checkout's like any
/// other; the skills root a run protects stays read-only as the deeper entry.
fn filesystem(conf: &Confinement, cwd: &Path) -> toml::Table {
    let mode = |m: &str| toml::Value::String(m.into());
    let writes = conf.writes_worktree();
    let mut fs = toml::Table::new();
    fs.insert(":tmpdir".into(), mode("write"));
    fs.insert(":slash_tmp".into(), mode("write"));
    // Named explicitly: `:slash_tmp` opens all of /private/tmp, so a checkout
    // there needs its own entry.
    fs.insert(
        cwd.display().to_string(),
        mode(if writes { "write" } else { "read" }),
    );
    if writes {
        fs.insert(cwd.join(AGENTS_DIR).display().to_string(), mode("write"));
    }
    fs.extend(
        conf.roots
            .iter()
            .map(|root| (root.display().to_string(), mode("write"))),
    );
    fs.extend(
        conf.protected_trees(cwd)
            .iter()
            .map(|tree| (tree.display().to_string(), mode("read"))),
    );
    fs
}

/// The probe's report with the CLI's version held to `VERSION_FLOOR`: a
/// version below the floor, or one that does not read as three numbers,
/// makes the report unhealthy, since the profile's keys are unknown to it.
pub(super) fn held_to_floor(report: ProbeReport) -> ProbeReport {
    match report {
        ProbeReport::Healthy { version } => {
            let shortfall = floor_shortfall(version.as_deref().unwrap_or_default());
            match shortfall {
                Some(diagnostic) => ProbeReport::Unhealthy { diagnostic },
                None => ProbeReport::Healthy { version },
            }
        }
        unhealthy => unhealthy,
    }
}

/// Why `reported` does not meet `VERSION_FLOOR`, or `None` when it does.
fn floor_shortfall(reported: &str) -> Option<String> {
    let Some(found) = reported_version(reported) else {
        return Some(format!(
            "the Codex CLI reports `{reported}`, which does not read as a version number"
        ));
    };
    if found >= VERSION_FLOOR {
        return None;
    }
    let (major, minor, patch) = VERSION_FLOOR;
    Some(format!(
        "the Codex CLI reports `{reported}`, older than {major}.{minor}.{patch}, \
         the lowest version that honors the session profile"
    ))
}

/// The `(major, minor, patch)` a version report names: the last token of
/// `codex-cli <v>`, when it is three numbers joined by dots.
fn reported_version(reported: &str) -> Option<(u64, u64, u64)> {
    let numbers = reported
        .split_whitespace()
        .last()?
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<u64>>>()?;
    match numbers.as_slice() {
        [major, minor, patch] => Some((*major, *minor, *patch)),
        _ => None,
    }
}
