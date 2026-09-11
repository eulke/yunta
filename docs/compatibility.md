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
  `publisher/name`. Every run yunta creates — a `yunta run`, a child of a
  `kind: workflow` node, a promotion successor — gets a ULID as its id: 26
  Crockford base32 characters that sort by the instant of birth. The link
  between a parent and its child, and between a run and its promotion
  successor, is recorded in the log (`child_run_created`, `promoted_from`),
  never encoded in the id.
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

`node_failed` records what failed, not a sentence about it. Its `failure` is
either `outcome:` — one sentence the engine states, for a failure with no
document behind it — or `artifacts:`, one entry per declared artifact that did
not close. An entry is either the file itself (`artifact-missing`,
`artifact-empty`, `artifact-oversized` with both numbers, `artifact-unreadable`)
or its content: the path, the kind whose shape it was read against, and every
problem that document has. Each problem names its subject in the document's own
words — ``task `t1`, criterion 1`` — which is the whole of where it is: a
diagnostic carries no line and column, and `events.json` publishes none. Every
surface renders from that value — `status`, the receipt, and the instruction a
repair attempt gets — so none of them can disagree about the facts, and nothing
has to take a sentence apart to recover them.

A log whose `node_failed` events carry `outcome:` on its own — every log written
before `artifacts:` existed — reads back as exactly that one-sentence failure:
no migration, nothing inferred. That tolerance is the rule for everything the
engine persists and reads again. The other direction is the general rule above:
a payload a binary does not recognize as that kind's shape is corrupt to it.

Because each document travels with its own problems, a count says which document
each came from: the receipt counts a problem by its code together with the kind of
document it was found in, and a failure of the file itself, which has no document,
counts by code alone.

`yunta list --runs` orders runs by the timestamp of their first event.

## The JSON surfaces

`stats --json`, `status --json` and `run --json` carry `schema_version: 2`. The
three share one stamp, so all of them carry the new number even though only
`status --json` changed shape.

In `status --json`, `diagnostics` maps a failed node to the documents its failure
names — `{"<node>": [{path, kind?, diagnostics?, file?}, ...]}`, one entry per
file. `path` is always there. A content failure carries `kind`, the artifact kind
whose shape the file was read against, and `diagnostics`, every problem that
document has in document order. A file-level failure carries `file` instead,
naming what went wrong with the file itself: never written, empty, past
`limits.max_artifact_bytes`, or refused by the filesystem. A node whose most
recent failure is a plain message has no entry at all, so what the field shows is
always the state the node is in now.

`artifact_checked` is a new event kind, written by a session through
`yunta_check_artifact` rather than by the engine. It carries `name`, the
`artifact_kind` when the artifact is interpreted, and a `verdict` of `ok` or
`problems` with the `codes` the check named. Adding a kind is the compatible
kind of change: a reader that does not know it derives what it can and marks the
run partially interpreted, and a log written before it simply has none.

The receipt gains `self_checks`: how many checks a run ran, how many answered
clean, what the sessions corrected in place, and which nodes produced an
interpreted artifact without ever checking one. Those corrections appear nowhere
else — a session that fixes its own file before closing leaves no failure behind
— so a receipt from an older version under-reports what a run got wrong, rather
than disagreeing with a newer one.

## Message wording

The block that reports what is wrong with a document counts in whole words —
`1 error`, `2 errors`. Every surface that prints it says it the same way: an
interpreted artifact that could not be read, a workflow that fails `yunta check`
(``the workflow fails `yunta check`: 2 errors``), `yunta new`, `yunta pack new`.

`yunta run` and `yunta resume` refuse an unhealthy adapter with ``adapter health
check failed (run `yunta doctor` for detail): 2 errors`` — the advice sits inside
the parenthesis so the count lands directly after the heading.

The `document_shape` tool refuses an unknown kind with the same sentence
`yunta schema` prints, byte for byte.

A run the binary could only interpret in part counts the same way on both surfaces
that report it: `2 unknown event kinds, interpreted partially: <kind> ×<count>, …`,
and `1 unknown event kind` for one. `yunta status` folds that into its
`·`-separated summary; `yunta stats` gives it a line of its own.

`yunta test` closes a case with what it found: `case <name> ... FAILED: 2 errors`,
`case <name> ... ERROR: 1 error`, and `case <name> ... ok` on its own. The tally
under them counts cases — `4 cases, 1 failed`, `1 case, 1 failed` — and carries no
error count, unlike every other heading that introduces problems: `failed` counts
cases while the lines beneath it count problems, and one failing case contributes
several, so a count there would put two different totals on one line. Each count
stays with what it counts.

`yunta pack add --run-tests` and `yunta pack audit` report that same tally in their
tests section — `tests: 4 cases, 1 failed`, or `tests: 4 cases shipped, not run
(pass --run-tests)` when nothing ran, or `tests: none shipped` when the pack ships
no cases. Failure lines sit two spaces in under it.

`yunta pack audit` prints a node's `prompt:` block even when the prompt file is
empty: a blank line under the heading, an empty block that shows it is empty.

## The schemas as files

`crates/core/schemas/` holds `workflow.json`, `config.json`, `pack.json`,
`ledger.json`, `findings.json`, `questions.json` and `events.json`: the JSON
Schema (draft 2020-12) of a workflow file, a config layer, a pack manifest, the
three artifacts the engine interprets, and one event of the log — the shape of a
line of `events.jsonl`. They are generated from the types that read those
documents: `cargo xtask schema` writes them and CI fails when a committed file
differs from what the types emit, so any change to a format is a visible diff in
the pull request that makes it. They live inside the crate whose types produce
them, which is also the crate that ships them: the binary embeds those exact
files, so `yunta schema <kind> --json` prints the bytes CI checked rather than
deriving a schema of its own at run time. An editor or a validator can use the
files as they are, with or without a checkout.

## Platforms

Yunta builds and is published for Linux and macOS. The engine's process
layer — every subprocess in its own process group, interrupted and killed
with its whole tree, lock liveness checked by signal — is POSIX, and
`yunta-engine` refuses to compile for any other target rather than ship a
binary that would leave processes behind on cancellation. Windows becomes a
target when a process layer with the same guarantees exists and its
cancellation tests pass on a Windows runner.

A lock's holder is asked about by signal on every platform, and a holder
that cannot be asked about — a process of another user — keeps its lock.
On Linux the holder's start time, read from `/proc`, also tells a reused
pid from the holder, so a newcomer that got a dead holder's pid cannot keep
its lock. macOS publishes no `/proc`: there liveness alone decides, and a
pid reused while the lock stands keeps it until that process ends.

## What isn't covered here

Individual adapters (CLI integrations like `claude-code`, `codex`) have
their own version compatibility against the coding-agent CLI they wrap; see
`yunta doctor`, which checks the installed binary's version against what the
adapter supports. Pack compatibility (a pack's own `declares:`/`requires:`
against a given Yunta version) is the pack author's responsibility, checked
statically at `pack add`/`check` time — this document covers the engine
itself, not third-party content distributed through it.
