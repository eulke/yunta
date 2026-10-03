//! The ecosystems whose manifest says enough on its own: Rust, Go,
//! Python. Their commands are the ecosystem's own conventions.

use std::path::Path;

use super::{commands, Found};

/// An ecosystem a repository is in, and what to know about building in
/// one checkout per task there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ecosystem {
    pub(crate) name: &'static str,
    /// A tip printed to the terminal, never written to config — no key
    /// for it exists in the schema.
    pub(crate) cache_tip: &'static str,
}

/// A repository with a `Cargo.toml` is primarily a Rust one, whatever
/// small tool beside it carries a `package.json`.
pub(super) fn rust(repo: &Path) -> Option<Found> {
    repo.join("Cargo.toml").is_file().then(|| Found {
        ecosystem: Ecosystem {
            name: "rust",
            cache_tip: "share a build cache across worktrees: export \
                        CARGO_TARGET_DIR=$HOME/.cache/yunta-cargo-target before running yunta \
                        — otherwise every worktree rebuilds the whole dependency tree.",
        },
        commands: commands(&[
            (
                "lint",
                "cargo clippy --all-targets -- -D warnings".to_string(),
            ),
            ("test", "cargo test".to_string()),
            ("format", "cargo fmt --check".to_string()),
        ]),
        suite: Some("cargo test".to_string()),
    })
}

pub(super) fn go(repo: &Path) -> Option<Found> {
    repo.join("go.mod").is_file().then(|| Found {
        ecosystem: Ecosystem {
            name: "go",
            cache_tip: "Go's own build and module caches (GOCACHE/GOMODCACHE) are already \
                        shared machine-wide by default — nothing extra to configure for \
                        worktrees.",
        },
        commands: commands(&[
            ("lint", "go vet ./...".to_string()),
            ("test", "go test ./...".to_string()),
        ]),
        suite: Some("go test ./...".to_string()),
    })
}

/// Pytest for the suite; a lint only where the project configures ruff.
pub(super) fn python(repo: &Path) -> Option<Found> {
    let manifest = std::fs::read_to_string(repo.join("pyproject.toml")).ok()?;
    let mut found = vec![("test", "pytest".to_string())];
    if manifest.contains("[tool.ruff") {
        found.push(("lint", "ruff check .".to_string()));
    }
    Some(Found {
        ecosystem: Ecosystem {
            name: "python",
            cache_tip: "share a virtualenv or package cache across worktrees (a shared \
                        `uv`/`pip` cache dir) to avoid reinstalling dependencies per worktree.",
        },
        commands: commands(&found),
        suite: Some("pytest".to_string()),
    })
}
