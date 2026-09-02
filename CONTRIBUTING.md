# Contributing to Yunta

Yunta is a deterministic workflow engine: state is derived by replaying an
append-only event log, and a workflow's own `criteria` are the only thing that
marks work done. The same principle governs how the project itself is built.
`CLAUDE.md` is the reference of judgment for anyone working in this
repository, human or agent; read it first. Everything below is logistics.

## Before you start

For anything beyond a small, obviously-scoped fix, open an issue first
describing the problem and the approach. Design decisions live in
`docs/design/adrs.md`; a change that touches one starts as a proposal in
`docs/design/adr/` and is settled there before any code.

## Building and testing

```bash
cargo build --workspace
cargo test --workspace                 # the whole suite runs against the mock adapter
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p yunta -- test             # the repository's own workflow cases
```

The toolchain is pinned in `rust-toolchain.toml`; rustup installs it on the
first build.

CI runs the same checks plus `cargo deny check`, an isolated `cargo check` per
crate and the self-tests of every pack under `packs/`.

## Pull requests

One topic per pull request, conventional commit messages, and a description
that says what changes and, when the diff does not make it obvious, why. A pull
request is done when every check in CI is green and every point of
`CLAUDE.md`'s definition of done holds.

## Reporting a security issue

Report vulnerabilities privately, as described in `SECURITY.md`.
