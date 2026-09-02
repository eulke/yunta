# Compatibility policy

This is the published guarantee for what changes, and what doesn't, across a
Yunta release. It exists so a team adopting Yunta can answer "will upgrading
break our workflows or our in-flight runs" without reading the changelog
line by line.

## The binary: semver

The `yunta` binary and the `yunta-core`/`yunta-storage`/`yunta-adapters`/
`yunta-engine` library crates follow [semver](https://semver.org/):

- **Patch** (`0.1.x`) — bug fixes, no behavior a workflow author could have
  depended on changes.
- **Minor** (`0.x.0`) — new node kinds, context sources, CLI flags, adapter
  capabilities. Additive; nothing that worked before stops working.
- **Major** (`x.0.0`) — anything that isn't additive: a removed flag, a
  changed default, a stricter `check` that now rejects something it used to
  accept.

Before `1.0.0`, minor bumps may still include breaking changes — the same
convention every pre-1.0 Rust crate uses — but the same three-tier reasoning
applies to decide which digit moves.

## The workflow schema: `yunta_schema`, versioned independently

A workflow's own format is versioned separately from the binary, via the
`yunta_schema:` field (a semver range, e.g. `yunta_schema: ">=1 <2"`) that
`yunta check` validates against the binary's own compiled schema version
(`yunta_core::YUNTA_SCHEMA`). This is deliberate: **upgrading the binary
must never retroactively invalidate a workflow a team already wrote and is
running in production.**

The engine supports the current schema version and the one immediately
before it (**N and N-1**). A schema bump (a new required field, a changed
node-kind shape) ships in a minor or major release alongside that release's
own notes on what changed and how to migrate; workflows on the outgoing
version keep working for one more schema generation before `check` starts
rejecting them, with an error that names the field and the fix.

## In-flight runs are never affected by an upgrade

A run's [manifest](guide.md) — workflow, config, inputs, and resolved
runners — is hashed and frozen at creation. Upgrading the `yunta` binary mid-run, or between `yunta run`
and a later `yunta resume`, changes nothing about how that run's remaining
nodes execute: `resume` replays the run's own event log against its own
frozen manifest, never against whatever the newly-installed binary would
generate today. The only thing a new binary version can change for an
existing run is how `status`/`stats`/`graph` *render* information already in
the log — never the log's content or the run's outcome.

## What every release verifies before it ships

Per the release pipeline's `test` gate (`.github/workflows/release.yml`):

- `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`,
  `cargo fmt --all --check`, `cargo deny check` — all green.
- The factory packs (`packs/starter`, `packs/fragua`) pass `yunta check` and
  their own `.yunta/tests/` cases against the `mock` adapter
  (`yunta test --dir <pack>`).
- Each of the four published binaries (Linux x86_64 and aarch64 as static
  musl binaries, macOS x86_64 and aarch64) installs and runs in a clean
  environment for its target platform, and `yunta doctor` is the first
  command the installer suggests running.

None of this is negotiable on a per-release basis — a release that doesn't
pass all of it doesn't ship.

## Platforms

Yunta builds and is published for Linux and macOS. The engine's process
layer — every subprocess in its own process group, interrupted and killed
with its whole tree, lock liveness checked by signal — is POSIX, and
`yunta-engine` refuses to compile for any other target rather than ship a
binary that would leave processes behind on cancellation. Windows becomes a
target when a process layer with the same guarantees exists and its
cancellation tests pass on a Windows runner.

## What isn't covered here

Individual adapters (CLI integrations like `claude-code`, `codex`) have
their own version compatibility against the coding-agent CLI they wrap; see
`yunta doctor`, which checks the installed binary's version against what the
adapter supports. Pack compatibility (a pack's own `declares:`/`requires:`
against a given Yunta version) is the pack author's responsibility, checked
statically at `pack add`/`check` time — this document covers the engine
itself, not third-party content distributed through it.
