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

Each release publishes a binary for Linux (x86_64 and aarch64, static musl) and
macOS (x86_64 and aarch64); Yunta isn't published to crates.io.
[`install.sh`](install.sh) installs the latest release's binary for your platform to
`~/.local/bin` (or `$YUNTA_INSTALL_DIR`), no sudo, once it matches the release's
SHA-256 checksums; `YUNTA_VERSION=v0.0.5` pins a release:

```bash
curl -fsSL https://raw.githubusercontent.com/eulke/yunta/main/install.sh | sh
```

Or, from a clone already on disk, `./install.sh`. Where no release matches the
version and the platform, the script builds a static binary from source instead,
which needs a recent stable Rust toolchain. On Linux that build links against `musl`
for a real static binary (no glibc dependency at runtime) — `rustup target add
x86_64-unknown-linux-musl` plus a musl C toolchain (`apt install musl-tools` on
Debian/Ubuntu) if you don't have one already; the script installs the Rust target
itself.

For local development, plain `cargo build --release` (binary at
`target/release/yunta`) or `cargo run -p yunta --` from the workspace root both work
without installing anything.

## Quickstart: a three-node workflow from scratch

This walks through writing a workflow file by hand — no pack, no config beyond the
one file `yunta init` writes. It's the whole mental model: nodes, dependencies, a
failure reroute, and verification that means something.

**1. Point Yunta at a project.**

```bash
yunta init
```

This detects your language, the commands it runs for lint, tests and the rest, the
forge your `origin` is on, and your base branch, and writes `.yunta/config.yaml`. It never overwrites an existing one without `--force`.

**2. Write the workflow.** Create `.yunta/workflows/lint-fix.yaml`:

```yaml
name: lint-fix
description: "Lint the workspace; if it fails, an agent fixes it and lint re-runs."

nodes:
  - id: lint
    kind: bash
    run: "cargo clippy --workspace -- -D warnings"
    on_failure: { goto: fix-lint, max_reroutes: 1 }

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
  failure, `on_failure.goto` reroutes control to `fix-lint` instead of failing the
  run; `max_reroutes` caps how many correction attempts happen before the run
  escalates to a person instead of looping forever.
- `fix-lint` is `kind: prompt` — an agent session. `scope` is a hard boundary: any
  edit outside `src/**` fails the node, even if the agent thinks it's related. There's
  no `depends_on` here because reaching this node *is* the dependency: a node named
  only as an `on_failure.goto` target starts exclusively through that reroute, never
  on its own — if `lint` passes, `fix-lint` never opens a session at all.
- `tests` is `kind: bash` again, gated by `depends_on: [lint]` — it only runs once
  `lint` has actually passed (whether on the first try or after a fix-and-retry).

**3. Name a runner.** `fix-lint`'s `runner: executor` needs at least one adapter
candidate in config, even though it may never actually run. Add one to
`.yunta/config.yaml` — `yunta init` ends with this step, naming the adapter CLIs
it found on your machine, and `yunta init -i` asks each runner's adapter and
model and writes it declared:

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
yunta run .yunta/workflows/lint-fix.yaml
```

`run` shows the run live while it works: a pinned region at the bottom of the
terminal — a row for each node at work, what its session last said and what its last calls touched, and a footer
that always says whether the run needs you or is waiting for a limit's reset — with each finished node scrolling above
it into your own scrollback, where you can still scroll back through it and select
text. A node that fails brings the end of what it printed with it. Piped into a file
or running in CI, every moment of the run arrives as one line instead, in the words
the live view uses. `--quiet` cuts it down to the run id on stdout and one line of
verdict on stderr, and still gives the budget warning that asks you to decide before
the run spends.

If `lint` passes clean — likely, for a freshly generated project — the run finishes
having only ever run `lint` and `tests`; `fix-lint` stays untouched, no session opened,
no tokens spent. Break the lint step on purpose and run it again to see `fix-lint`
actually open a real session and fix it.

**6. Check on it later.**

```bash
yunta status last
```

`yunta` alone shows every run of the repository you are in, what needs you first;
outside a repository it says so, and what to run instead.
A run is called by its handle — the last six characters of its id, which every
line you read prints — and `status`, `resume` and every other command that takes a
run take it back, along with the whole id, any part that starts or ends it, `last`
(this repository's newest run) and `needs` (the one waiting on you).

`status` derives everything it prints from the event log — node states, which task
ran, token counts — never from an agent's self-report. If the run is mid-flight or
waiting on a gate's decision, `resume` and `resolve-gate` pick it back up from exactly where the
log left off, in a different process if you like; nothing about a run depends on
the terminal that started it staying open.

That's the whole loop. Real workflows add more node kinds (`loop` over a tasks document,
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
a verified tasks document, lint→fix, a baseline check, multi-runner review, the
work held to its plan, PR).

## Commands

| Command | Does |
|---|---|
| `yunta init [-i] [--force]` | Detects language, the project's commands and suite, the forge `origin` is on, base branch and available adapters; writes `.yunta/config.yaml`. `-i` asks to confirm the project's name and base branch, then each runner the workflows name: its adapter, among those that answered, and its model; a runner whose model is left empty stays commented, to declare later. |
| `yunta new <name> [--shape one-node\|lint-fix\|tasks] [-i] [--force]` | Writes a commented workflow skeleton to `.yunta/workflows/<name>.yaml` and reports the verdict `check` reaches on it, exit code included. A `lint-fix` skeleton runs the project's own `lint` command when its config declares one. |
| `yunta schema [<document>] [--json]` | The shape of a document Yunta reads and validates — `tasks`, `spec`, `findings`, `questions`, `answers` — as an annotated example to copy, or as JSON Schema for an editor; `workflow` lists every key a workflow and each kind of node read. With no arguments, lists the documents. Nothing has to be set up first: this is how anyone who has to produce one of these documents, agent or person, learns the shape instead of guessing it. |
| `yunta check <workflow> [--config <path>]` | Validates a workflow statically: cycles, unreachable reroutes, undefined runners, template variables, permission ceilings — no session opened. `--config` checks it against another config file. |
| `yunta run <workflow> [--input k=v] [--adapter <id>] [--fixture <path>] [--mode] [--quiet] [--detach] [--json]` | Creates a run from a workflow and executes it, showing it live on a terminal: a pinned region of text, with finished work scrolling above it into your own scrollback. Without a terminal — a pipe, CI, `TERM=dumb` — every moment of the run arrives as one line, in the words the live view uses, and the first line says why. `--quiet` cuts the output to the run id on stdout and one line of verdict on stderr, and still gives the budget warning that asks for a decision before the run spends. `--adapter` runs every session on that adapter, through every resume of the run: the pin is frozen with the run, so a resume, a detached child or a successor takes the same one. Each role resolves to its candidate on it, and the log records the candidates passed over; a role with no candidate on it refuses the run before it is created, naming the node and the role. `--adapter mock --fixture <path>` runs against a scripted fixture with no LLM, and a resume plays on that fixture. `--detach` returns the run id immediately and keeps running independent of the calling process, and `yunta status <run> --follow` follows it from any terminal; `--json` prints the outcome as one versioned JSON document instead of the live view. |
| `yunta list [--runs [--all] [--json] [--full] [--dig]]` | Without `--runs`: the workflow catalog (repo + packs), each workflow with its description, one line per declared input, an estimate when history supports one, and the command that runs it. With `--runs`: this repository's runs as a field — `yunta` alone, in a repository, shows the same — headed by how many need you and how many stand anywhere else, then a group to each thing you can do about them: a run asking at its terminal now, the runs waiting on a decision, the stalled, the ones you stopped and the ones still moving, each on one line with its handle, workflow and mode, a short furrow of its nodes, what holds it, and how long it has been there or the command that moves it on, the longest-waiting first. What has closed, and the runs that do not read back, are counted under it; `--full` lists them, `--dig` names each run by its whole id. `--all` takes every run on the machine instead of this repository's. With `--runs`, `--json` prints the same inbox as one JSON document. |
| `yunta status <run> [--full] [--dig] [--node <id>] [--json] [--follow]` | A run's derived state, reconstructed from the event log, at a glance: the workflow, the word the run is called by and how long it has stood there, a furrow with a stroke for each node, what it asks of you, the evidence of what failed, and how far it has come — its nodes and tasks by where they stand. A run parked on a decision ends with the decision: what weighs on it, every option with its tradeoff and the `resolve-gate` command that answers it, then what moves the run without choosing. A run whose engine is asking at its terminal says it needs you, and where to answer. `--full` tells the whole story: every node with what it said or the plan it accepted, how long it worked and what it spent, every task, the runs it composed, its tokens and every other fact; `--dig` adds its whole id, where it is kept and the files that hold whole outputs. `--node` shows one node whole: its whole failure or what its agent said, the end of what it printed and where all of it is kept, and what it produced. A run parked on its budget carries the same caution here: `cautions` under `--json`, and the closing block. `--follow` follows a run that moves, from any terminal: it draws what `yunta run` draws, read from the run's log and, for what its sessions say, from the run's socket, ends with the same closing block when the run stops and exits with the same code; Ctrl-C stops following and leaves the run going. |
| `yunta open <run> [--json]` | Writes the run's page into its folder as `page.html` and opens it in a browser: one file, no server, its buttons copy commands. |
| `yunta resume <run> [--quiet] [--json]` | Resumes a run from its event log, restarting orphaned nodes per `on_interrupt`. Shows it exactly as `yunta run` does — the same live region, the same one line per moment without a terminal, the same closing block — and takes the same `--quiet` and `--json`: two commands that execute the same thing report it the same way. It refuses a run another engine is driving, and takes over from an engine that is gone. |
| `yunta resolve-gate <run> [<option>] [--by] [--text] [--json]` | Answers a paused run's gate decision from a separate process; hands the run to a detached resume that applies it. Without an option, puts the run's own menu to this terminal — and off a terminal, lists the command that chooses each option. A run stopped on its token cap is answered the same way: `continue` lets one engine go past the cap, and `abort` leaves the decision open. An option the gate withholds — going on with a plan that cannot be proven — is refused with why. `--json` prints the answer as one document; it needs an option, since the menu is for a terminal, so without one it is refused. |
| `yunta review <run>` | Reads the plan a parked run waits on at full screen, and answers it there; off a terminal, prints it with the commands that answer it. |
| `yunta cancel <run> [--json]` | Sends every running node's session an ordered interrupt, escalating to a full process-tree kill. A run `--detach` has just handed over is stopped once the engine it was handed to starts. `--json` prints the answer as one document. |
| `yunta close <run> [--by] [--json]` | Closes a stopped run nobody is going to continue, as cancelled and by who closed it, so it stops waiting on a person. A run whose engine died is settled first, as `cancel` settles one; its branch and worktree stay until `gc` removes them. `--json` prints the answer as one document. |
| `yunta graph <workflow> \| --run <id> [--format mermaid\|dot]` | Renders a workflow's DAG as Mermaid (or DOT): dependencies, reroutes, parallel groups drawn with their children inside them, gates. A path or a catalog name reads the workflow off disk; `--run <id>` draws the one that run froze, annotated with each node's derived state. A file that breaks its own rules is refused as every command refuses it; one that only the project's config cannot run is still drawn, with the verdict on stderr and in the exit code. |
| `yunta test [--dir <path>]` | Runs the cases under `.yunta/tests/` with the `mock` adapter — no LLM, no network, deterministic. |
| `yunta stats [<run>] [--workflow <name>] [--full \| --dig] [--json]` | Verification cost: cost-per-verified-task, rework rate, cache rate, wall-clock breakdown (time the host was suspended left out, and said) — for one run or a workflow's whole history. For one run, where its time went, as a bar of its parts: working, rails, checks (and how much of them guards ran with the task's own criterion already red), measuring the suite while nothing else ran, waiting on you — from the moment the engine began asking — offline, parked, and the engine between steps; then each node's time and tokens, and each runner's. `--full` adds the run's rails and each node's blocked share and models; `--dig` keeps each node's id whole. With `--workflow <name>` and no history, `--json` prints the same document with `runs` empty, where the human view says there are no runs. |
| `yunta verify <run> [--json]` | Checks a run's evidence end to end, reporting the two guarantees apart: its event hash chain, recomputed from the log as persisted, and the bytes of every artifact that log accepted, read back and hashed against its own name. `--json` prints the report as one document. |
| `yunta receipt <run> [--json]` | Generates a Verified Work Receipt for a finished run — markdown + JSON, derived entirely from the event log, written to the run's own directory; a run that failed names each node that failed and why. A `pull_request` node puts the same receipt in the pull request it opens. |
| `yunta doctor [--session] [--full] [--json]` | Health-checks every adapter your `runners:` name — the binary there and answering `--version`, every setting read — or, when it names none, gives the runner to declare on the adapters this machine answers for. At a glance the checks that hold fold into one row that names each, and every other check has its own row with what it means and how to make it hold; `--full` gives every check its own row, who git commits a run's work as among them. It checks every installed pack's `requires:` against your config: runners resolvable, mcp servers defined, commands on `PATH`. It fails only on what would stop a workflow in the catalog: a runner one needs, a forge token one opening a pull request needs. `--session` goes further and opens one real session per binding, which is the only way to find out whether a CLI accepts the configuration a run writes it; it spends a prompt each. `--json` prints the report as one document. |
| `yunta pack add <source>[@ref] [--yes] [--run-tests]` / `new <publisher>/<name>` / `update <publisher>/<name> <ref> [--yes]` / `remove <publisher>/<name>` / `list` | Clones, vendors and locks a third-party pack under `.yunta/packs/`, `yunta.lock` tracking exactly what's installed; nothing of the pack runs unless `--run-tests` asks for its own cases after the install. Its workflows and skills are then addressable as `publisher/name` (`yunta run acme/review`, `use: acme/qa-review`, `skills: [acme/rubric]`) — see [packs](docs/packs.md). `permissions.packs` governs both verbs: a non-empty publisher allow-list restricts sources, and the executors policy (`prompt` default: `--yes` to confirm; `deny`: refused outright; `allow`: no confirmation) gates packs that ship executable code. `check` refuses any node that exceeds the pack's own declared permissions ceiling. |
| `yunta pack audit <publisher>/<name>` | Prints a full static inventory of a pack's own workflows — every command, context source, per-node permission, agent, mcp server, executor and full untrimmed prompt — plus whether it ships tests and whether they pass. `add` runs this automatically before vendoring. |
| `yunta mcp` | Runs the MCP control plane over stdio: `document_shape`, `list_workflows`, `run_workflow`, `workflow_status`, `resume_run`, `resolve_gate`, `answer_questions`. `resolve_gate` takes what `yunta resolve-gate` takes, so an agent can authorize going past a token cap as a person can. |
| `yunta gc [--dry-run]` | Removes orphaned run and worktree directories, respecting `storage.retention_days`. |
| `yunta completions <shell>` | Prints the script that completes `yunta`'s commands and flags in `bash`, `zsh`, `fish`, `powershell` or `elvish` — e.g. `yunta completions zsh > ~/.zfunc/_yunta`. |

Every command takes `--color auto|always|never`. `auto` colors a stream that is a
terminal, unless `NO_COLOR` is set or `CLICOLOR=0`; `CLICOLOR_FORCE` colors a pipe
too. Color only repeats what the words say, so nothing is lost without it. A
terminal that announces it opens links gets the file a failure's whole output is
kept in as one; `YUNTA_HYPERLINKS=1` or `0` says so outright.
A line is laid out to its terminal's width, held between 60 and 120 cells;
`COLUMNS` sets it, and a pipe gets 80.

Every command's own `--help` is the source of truth for flags; this table is for
finding the right one.

## A run's lifecycle

A run is a workflow plus a manifest (workflow + config + inputs + resolved runners,
hashed and frozen at creation) plus an append-only event log in a local SQLite file.
Nothing about a run's state lives anywhere else — `status`, `resume` and `stats` all
derive their answer by replaying that log, which is what makes killing the process at
any point and resuming safe.

A node that has not started reads `never ran`, one at work reads `running`, and it ends
`finished`, `failed` or `skipped`. A node held by a gate — a human decision point — or by
questions nobody answered yet reads `needs you`, with nothing running underneath it; it
survives the engine restarting just
like any other state. `waiting` is only ever a clock: a node held until a limit resets. A
node whose run was stopped while its attempt was open reads `stopped`: nothing runs it, and
the run's resume starts it again.
A failed node either ends the run or, if it declared `on_failure.goto`, reroutes to a
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
