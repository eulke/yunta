# Yunta

Yunta is a deterministic workflow engine for code agents. Workflows are declared in
YAML; the engine runs them with mechanical verification, an append-only event log as
external state, and full auditability. An agent never marks its own work done — a
workflow's `criteria` do, by running and exiting `0`.

- **Deterministic.** State is derived by replaying the event log, never trusted from
  an agent's word. Kill the engine mid-run, run `yunta resume`, and it reaches the
  same final state.
- **Verified, not vibes.** Every task runs its criteria red-before-green: a criterion
  that already passes before the work starts proves nothing, so the engine rejects it.
- **No LLM required to test it.** The `mock` adapter scripts an agent's turns from a
  fixture, so `yunta test` and CI run the whole engine without ever calling a model.
- **Runs without a server.** Everything above is a local binary plus a SQLite file —
  no daemon, no account, no network dependency to run a workflow.

## Install

Yunta isn't published to crates.io or as a prebuilt release binary yet.
[`install.sh`](install.sh) builds a static binary from source and installs it to
`~/.local/bin`, no sudo:

```bash
curl -fsSL https://raw.githubusercontent.com/eulke/yunta/main/install.sh | sh
```

Or, from a clone already on disk, `./install.sh`. On Linux it links against `musl`
for a real static binary (no glibc dependency at runtime) — `rustup target add
x86_64-unknown-linux-musl` plus a musl C toolchain (`apt install musl-tools` on
Debian/Ubuntu) if you don't have one already; the script installs the Rust target
itself. Requires a recent stable Rust toolchain either way.

For local development, plain `cargo build --release` (binary at
`target/release/yunta`) or `cargo run -p yunta --` from the workspace root both work
without installing anything.

## Quickstart: a three-node workflow from scratch

This walks through writing a workflow file by hand — no pack, no config beyond the
one file `yunta init` writes. It's the whole mental model: nodes, dependencies, a
failure re-route, and verification that means something.

**1. Point Yunta at a project.**

```bash
yunta init
```

This detects your language and test command, finds your base branch, and writes
`.yunta/config.yaml`. It never overwrites an existing one without `--force`.

**2. Write the workflow.** Create `.yunta/workflows/lint-fix.yaml`:

```yaml
name: lint-fix
description: "Lint the workspace; if it fails, an agent fixes it and lint re-runs."

nodes:
  - id: lint
    kind: bash
    run: "cargo clippy --workspace -- -D warnings"
    on_failure: { goto: fix-lint, max_reroutes: 2 }

  - id: fix-lint
    kind: prompt
    runner: executor
    scope: ["src/**"]
    prompt: "Fix exclusively the clippy errors `lint` reported. Don't touch anything else."

  - id: tests
    kind: bash
    depends_on: [lint]
    run: "cargo test --workspace"
```

Three nodes, three node kinds:

- `lint` is `kind: bash` — a shell command whose exit code *is* the verdict. On
  failure, `on_failure.goto` re-routes control to `fix-lint` instead of failing the
  run; `max_reroutes` caps how many correction attempts happen before the run
  escalates to a person instead of looping forever.
- `fix-lint` is `kind: prompt` — an agent session. `scope` is a hard boundary: any
  edit outside `src/**` fails the node, even if the agent thinks it's related. There's
  no `depends_on` here because reaching this node *is* the dependency: a node named
  only as an `on_failure.goto` target starts exclusively through that re-route, never
  on its own — if `lint` passes, `fix-lint` never opens a session at all.
- `tests` is `kind: bash` again, gated by `depends_on: [lint]` — it only runs once
  `lint` has actually passed (whether on the first try or after a fix-and-retry).

**3. Name a runner.** `fix-lint`'s `runner: executor` needs at least one adapter
candidate in config, even though it may never actually run. Add one to
`.yunta/config.yaml` — whichever adapter `yunta doctor` reports healthy for you:

```yaml
runners:
  executor:
    - { adapter: claude-code, model: claude-sonnet-4-6 }
```

**4. Check it before spending a token.**

```bash
yunta check .yunta/workflows/lint-fix.yaml
```

`check` catches dependency cycles, unreachable `goto` targets, undefined runners and
template variables — all statically, without opening a single agent session.

**5. Run it.**

```bash
yunta run .yunta/workflows/lint-fix.yaml --follow
```

If `lint` passes clean — likely, for a freshly generated project — the run finishes
having only ever run `lint` and `tests`; `fix-lint` stays untouched, no session opened,
no tokens spent. Break the lint step on purpose and run it again to see `fix-lint`
actually open a real session and fix it.

**6. Check on it later.**

```bash
yunta status <run_id>
```

`status` derives everything it prints from the event log — node states, which task
ran, token counts — never from an agent's self-report. If the run is mid-flight or
paused on a gate, `resume` and `resolve-gate` pick it back up from exactly where the
log left off, in a different process if you like; nothing about a run depends on
the terminal that started it staying open.

That's the whole loop. Real workflows add more node kinds (`loop` over a task ledger,
`parallel` groups, `check` builtins, `gate` for human decisions, `workflow` to compose
other workflows) and richer context sourcing — all covered in the
[workflow guide](docs/guide.md). None of it requires a pack; [packs](docs/packs.md)
(`yunta pack add`) are for sharing workflows and knowledge across projects, not a
prerequisite for writing one. See the [documentation index](docs/README.md) for
everything else — concepts, adapters, and troubleshooting.

Two example packs ship in this repo's own [`packs/`](packs/) directory as
installable, removable third-party packs — the engine grants them no special
status: [`yunta/starter`](packs/starter) (two minimal workflows that teach the
shape) and [`yunta/fragua`](packs/fragua) (the full reference pipeline — grill,
a verified task ledger, lint→fix, a baseline check, multi-runner review, PR).

## Commands

| Command | Does |
|---|---|
| `yunta init` | Detects language, test command, base branch and available adapters; writes `.yunta/config.yaml`. |
| `yunta new <name> [--shape one-node\|lint-fix\|ledger]` | Writes a commented workflow skeleton to `.yunta/workflows/<name>.yaml` and checks it. |
| `yunta check <workflow>` | Validates a workflow statically: cycles, unreachable re-routes, undefined runners, template variables, permission ceilings — no session opened. |
| `yunta run <workflow> [--input k=v] [--adapter] [--mode] [--follow] [--detach]` | Creates a run from a workflow and executes it. `--follow` prints live progress; `--detach` returns the run id immediately and keeps running independent of the calling process. |
| `yunta list [--runs]` | Without `--runs`: the workflow catalog (repo + packs) with descriptions, inputs and modes. With `--runs`: local runs and their derived state. |
| `yunta status <run_id>` | A run's derived state: nodes, tasks, tokens — reconstructed from the event log. |
| `yunta resume <run_id>` | Resumes a run from its event log, restarting orphaned nodes per `on_interrupt`. |
| `yunta resolve-gate <run_id> <option> [--by] [--text]` | Answers a paused run's gate decision from a separate process; hands the run to a detached resume that applies it. |
| `yunta cancel <run_id>` | Sends every running node's session an ordered interrupt, escalating to a full process-tree kill. |
| `yunta graph <workflow\|run_id>` | Renders the DAG as Mermaid (or DOT): dependencies, re-routes, parallel groups, gates — annotated with derived state when given a run id. |
| `yunta test` | Runs the cases under `.yunta/tests/` with the `mock` adapter — no LLM, no network, deterministic. |
| `yunta stats [<run_id>] [--workflow] [--json]` | Verification cost: cost-per-verified-task, rework rate, cache rate, wall-clock breakdown — for one run or a workflow's whole history. |
| `yunta verify <run_id>` | Recomputes and checks a run's event hash chain end to end. |
| `yunta receipt <run_id> [--json]` | Generates a Verified Work Receipt for a finished run — markdown + JSON, derived entirely from the event log, written to the run's own directory. |
| `yunta doctor` | Health-checks every adapter your `runners:` name — binary present, version compatible, auth valid — and every installed pack's `requires:` against your config: roles resolvable, mcp servers defined, commands on `PATH`. |
| `yunta pack add <source>[@ref] [--yes]` / `update <publisher>/<name> <ref> [--yes]` / `remove <publisher>/<name>` / `list` | Clones, vendors and locks a third-party pack under `.yunta/packs/`, `yunta.lock` tracking exactly what's installed. Its workflows and skills are then addressable as `publisher/name` (`yunta run acme/review`, `use: acme/qa-review`, `skills: [acme/rubric]`) — see [packs](docs/packs.md). `permissions.packs` governs both verbs: a non-empty publisher allow-list restricts sources, and the executors policy (`prompt` default: `--yes` to confirm; `deny`: refused outright; `allow`: no confirmation) gates packs that ship executable code. `check` refuses any node that exceeds the pack's own declared permissions ceiling. |
| `yunta pack audit <publisher>/<name>` | Prints a full static inventory of a pack's own workflows — every command, context source, per-node permission, agent, mcp server, executor and full untrimmed prompt — plus whether it ships tests and whether they pass. `add` runs this automatically before vendoring. |
| `yunta mcp` | Runs the MCP control plane over stdio: `list_workflows`, `run_workflow`, `workflow_status`, `resume_run`, `resolve_gate`. |
| `yunta gc [--dry-run]` | Removes orphaned run and worktree directories, respecting `storage.retention_days`. |

Every command's own `--help` is the source of truth for flags; this table is for
finding the right one.

## A run's lifecycle

A run is a workflow plus a manifest (workflow + config + inputs + resolved runners,
hashed and frozen at creation) plus an append-only event log in a local SQLite file.
Nothing about a run's state lives anywhere else — `status`, `resume` and `stats` all
derive their answer by replaying that log, which is what makes killing the process at
any point and resuming safe.

Each node moves through `pending → ready → running → done | failed | skipped |
waiting`. `waiting` is a node paused on a gate — a human decision point — with nothing
running underneath it; it survives the engine restarting just like any other state.
A failed node either ends the run or, if it declared `on_failure.goto`, re-routes to a
correction node and tries again, up to `max_reroutes` before escalating to a gate
instead of looping forever.

Isolation defaults to a `git worktree` per run, so concurrent runs against the same
repo never collide; `isolation: none` is available for the rare case where you want
to run directly against the working tree, under stricter preconditions (clean tree,
no concurrent run). `on_finish` cleans up the worktree and can distill artifacts back
into the repo's `knowledge/` layer for future runs to draw on.

See the [workflow guide](docs/guide.md) for the full node-kind reference, context
sources, permissions, gates, and the authoring patterns (criteria granularity, shared
build caches across worktrees) worth knowing before writing a real workflow.

## Status

Yunta is under active development. It builds itself with itself: every commit in
this repo's history past the initial bootstrap was produced by a Yunta run against
its own codebase.
