//! JavaScript and TypeScript: the package manager a repository's
//! lockfile names, and the scripts its `package.json` declares for each
//! capability a workflow asks for.

use std::path::Path;

use serde_json::Value;

use super::ecosystems::Ecosystem;
use super::{commands, Found};

/// The scripts a workflow's capabilities are conventionally declared
/// under, the first a `package.json` has winning.
const SCRIPTS: &[(&str, &[&str])] = &[
    ("lint", &["lint"]),
    (
        "typecheck",
        &["typecheck", "type-check", "check-types", "tsc"],
    ),
    ("test", &["test"]),
    ("format", &["format:check", "fmt:check", "format"]),
    ("build", &["build"]),
];

pub(super) fn node(repo: &Path) -> Option<Found> {
    let text = std::fs::read_to_string(repo.join("package.json")).ok()?;
    let manifest: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let manager = PackageManager::of(repo, &manifest);
    let declared = manifest.get("scripts").and_then(Value::as_object);
    let found: Vec<(&'static str, String)> = SCRIPTS
        .iter()
        .filter_map(|(capability, names)| {
            let script = names.iter().find(|name| {
                declared
                    .and_then(|scripts| scripts.get(**name))
                    .and_then(Value::as_str)
                    .is_some_and(runs_something)
            })?;
            Some((*capability, manager.run(script)))
        })
        .collect();
    let suite = found
        .iter()
        .find(|(capability, _)| *capability == "test")
        .map(|(_, text)| text.clone());
    Some(Found {
        ecosystem: Ecosystem {
            name: "node",
            cache_tip: "share a package cache across worktrees: point the package manager's \
                        cache at a shared directory, or use one with content-addressed storage \
                        (pnpm).",
        },
        commands: commands(&found),
        suite,
    })
}

/// Whether a script does anything but fail: `npm init` writes a `test`
/// that only says none is specified, and a suite that always fails
/// measures nothing.
fn runs_something(script: &str) -> bool {
    !script.contains("no test specified")
}

/// Who runs a repository's scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    /// The one the lockfile names, else the one `packageManager` names,
    /// else npm.
    fn of(repo: &Path, manifest: &Value) -> Self {
        let lockfiles = [
            ("pnpm-lock.yaml", PackageManager::Pnpm),
            ("yarn.lock", PackageManager::Yarn),
            ("bun.lockb", PackageManager::Bun),
            ("bun.lock", PackageManager::Bun),
            ("package-lock.json", PackageManager::Npm),
        ];
        if let Some((_, manager)) = lockfiles.iter().find(|(file, _)| repo.join(file).is_file()) {
            return *manager;
        }
        let declared = manifest
            .get("packageManager")
            .and_then(Value::as_str)
            .unwrap_or_default();
        [
            ("pnpm@", PackageManager::Pnpm),
            ("yarn@", PackageManager::Yarn),
            ("bun@", PackageManager::Bun),
        ]
        .iter()
        .find(|(prefix, _)| declared.starts_with(prefix))
        .map_or(PackageManager::Npm, |(_, manager)| *manager)
    }

    /// The command that runs `script`.
    fn run(self, script: &str) -> String {
        match self {
            PackageManager::Npm if script == "test" => "npm test".to_string(),
            PackageManager::Npm => format!("npm run {script}"),
            PackageManager::Pnpm => format!("pnpm {script}"),
            PackageManager::Yarn => format!("yarn {script}"),
            PackageManager::Bun => format!("bun run {script}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a scratch repo");
        for (name, content) in files {
            std::fs::write(dir.path().join(name), content).expect("a file of the repo");
        }
        dir
    }

    fn found(files: &[(&str, &str)]) -> Vec<(String, String)> {
        let repo = repo_with(files);
        node(repo.path())
            .expect("a package.json")
            .commands
            .into_iter()
            .map(|(name, text)| (name.to_string(), text))
            .collect()
    }

    #[test]
    fn the_lockfile_names_the_package_manager() {
        let scripts = r#"{ "scripts": { "lint": "eslint .", "test": "vitest" } }"#;
        assert_eq!(
            found(&[("package.json", scripts), ("pnpm-lock.yaml", "")]),
            [
                ("lint".to_string(), "pnpm lint".to_string()),
                ("test".to_string(), "pnpm test".to_string())
            ]
        );
        assert_eq!(
            found(&[("package.json", scripts), ("yarn.lock", "")])[0].1,
            "yarn lint"
        );
    }

    #[test]
    fn npm_runs_a_script_by_name_but_tests_on_its_own() {
        let scripts = r#"{ "scripts": { "type-check": "tsc", "test": "jest" } }"#;
        assert_eq!(
            found(&[("package.json", scripts)]),
            [
                ("test".to_string(), "npm test".to_string()),
                ("typecheck".to_string(), "npm run type-check".to_string())
            ]
        );
    }

    #[test]
    fn the_test_npm_init_writes_is_no_test() {
        let manifest =
            r#"{ "scripts": { "test": "echo \"Error: no test specified\" && exit 1" } }"#;
        assert_eq!(found(&[("package.json", manifest)]), []);
    }

    #[test]
    fn package_manager_names_it_where_no_lockfile_does() {
        let manifest =
            r#"{ "packageManager": "pnpm@9.1.0", "scripts": { "build": "vite build" } }"#;
        assert_eq!(found(&[("package.json", manifest)])[0].1, "pnpm build");
    }
}
