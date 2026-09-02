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

## Scope globs

A scope pattern — a node's or task's `scope`, an expansion's `within` — reads
with a literal separator: `*` never crosses a `/`, so `src/*.rs` names the
files directly under `src/` and `src/**` names everything beneath it. A
workflow that expects `*` to descend into subdirectories writes `**`.

## Identifiers

Every identifier is checked when it is read, and a value that breaks its
rule is refused with the value, what it was meant to be and the rule:

- A node id, a runner name, a mode name, a task id, a question id, an
  adapter id and an executor name are a letter followed by letters, digits,
  `_` or `-`. A fan-out sibling the manifest expands `runners:` into adds
  `@` and its runner's name; authored YAML never spells that form.
- A model name and an agent name are one printable word without whitespace,
  as the adapter's CLI accepts them.
- A run id, a publisher and a pack name are one path segment: printable,
  without whitespace, `/` or `\`, and not `.` or `..`. A pack reference is
  `publisher/name`.
- A finding id is any printable label; a session id is whatever the
  adapter's CLI issued, as long as it is not empty.

A pack manifest names the runners it needs under `requires.runners`. The
`runner_resolved` event names its runner under `runner`; the log reader also
accepts `role`, the field's former name, so a log written under it still
replays. `stats --json` and the JSON receipt name the same value `runner`.

## The event log

The engine hands storage a draft — what happened, in which run, for which
node — and storage assigns the event's position (`seq`, from 1) and its
timestamp from the injected clock; no caller invents either.

A binary reads every log a newer binary wrote. An event under a `kind` this
binary does not know is kept as written — its kind, the version it was
written under and every field — and the run is interpreted up to what the
binary understands: replay never breaks on it, `yunta status` and the
receipt name the unknown kinds with their counts, `stats --json` lists them
under `unknown_kinds`, and `events.jsonl` carries the event back out
verbatim (with its `schema_version` beside the envelope fields). An event
under a known kind whose payload is not that kind's shape is corrupt, and
reading the run fails naming its position.

`yunta list --runs` orders runs by the timestamp of their first event.

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
