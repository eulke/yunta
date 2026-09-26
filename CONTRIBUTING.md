# Contributing to Yunta

[AGENTS.md](AGENTS.md) contains shared engineering principles. This page
lists local commands and contribution logistics.

## Building and testing

```bash
cargo build --workspace
cargo test --workspace                 # the whole suite runs against the mock adapter
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p yunta -- test             # the repository's own workflow cases
cargo xtask schema                     # regenerates crates/core/schemas/ after a change to a document type
cargo run -p xtask -- smells --check   # the measured smell counts have not risen
```

The toolchain is pinned in `rust-toolchain.toml`. See
`.github/workflows/ci.yml` for the current CI checks.

## Pull requests

Keep a pull request focused on one topic. Explain what changed, why, and what
evidence supports it. Use conventional commit messages and resolve CI
failures before merging.

## Reporting a security issue

Report vulnerabilities privately, as described in `SECURITY.md`.
