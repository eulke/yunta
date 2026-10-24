# Contributing to Yunta

[AGENTS.md](AGENTS.md) is the map a coding agent reads before changing
Yunta, with the engineering principles a contributor shares. This page lists
local commands and contribution logistics.

## Building and testing

```bash
cargo build --workspace
cargo test --workspace                 # the whole suite runs against the mock adapter
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p yunta -- test             # the repository's own workflow cases
cargo xtask schema                     # regenerates crates/core/schemas/ after a change to a document type
cargo run -p xtask -- smells --check   # no smell count rose above the baseline or the merge base's
cargo run -p xtask -- smells           # lowers the baseline to what the tree counts; refuses a rise
cargo run -p xtask -- docs --check     # the agent docs and the map hold their shapes
cargo run -p xtask -- docs --about <files>   # what the agent docs record about those files
```

A golden test compares a surface's whole output with a file under the crate's
`goldens/`, one per case and environment, escapes written as `␛`. When a change
means to change what a surface prints, write the new files and review them in
the diff:

```bash
YUNTA_BLESS=1 cargo test -p yunta -p yunta-engine -p yunta-render
git diff -- crates/cli/goldens crates/engine/goldens crates/render/goldens
```

The toolchain is pinned in `rust-toolchain.toml`. See
`.github/workflows/ci.yml` for the current CI checks.

## Releases

A release is a pushed `vX.Y.Z` tag. `.github/workflows/release.yml` runs every
step of the `ci` job on it, then builds the binary for each platform (Linux
x86_64 and aarch64 as static musl binaries, macOS x86_64 and aarch64) and
starts each one in a clean container or on a fresh runner. Only once all of
that passes does it publish the GitHub release, each binary with
`checksums.txt`, which `install.sh` verifies before it installs one.

## Pull requests

Keep a pull request focused on one topic. Explain what changed, why, and what
evidence supports it. Use conventional commit messages and resolve CI
failures before merging.

## Reporting a security issue

Report vulnerabilities privately, as described in `SECURITY.md`.
