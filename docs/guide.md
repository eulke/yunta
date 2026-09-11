# Workflow guide

The [README quickstart](../README.md#quickstart-a-three-node-workflow-from-scratch)
gets a `bash` → `prompt` → `bash` chain running. This is the reference for
everything past that: the other node kinds, context, permissions, gates, modes, and
two authoring patterns worth using from the start — criteria granularity and shared
build caches across worktrees.

Nothing here needs a pack installed. Packs (`yunta pack add`) are a
distribution mechanism for sharing workflows and knowledge across projects — every
mechanism below works the same in a single, pack-free repo. See [packs.md](packs.md)
for installing, creating, or publishing one.

## Node kinds

Every node has an `id`, an optional `depends_on: [ids]`, and one `kind`. An `id`
is a letter followed by letters, digits, `_` or `-` — the same rule names runners,
modes, tasks and questions. Every key is checked: a key the schema does not know —
a typo, or a key that belongs to another kind — is refused by `yunta check` and by
every command that reads the file, with the key, where it sits and the keys that
are valid there. A mistyped key never silently becomes a default.

- **`bash`** — `run: "<command>"`. Exit code is the verdict; no session, no agent.
- **`prompt`** — `prompt: "<text>"` (or `prompt: { file: path/to/prompt.md }` for
  longer ones). Opens one agent session. `runner:` picks which role from `runners:`
  in config resolves it; `permissions: read-only|edit|full` caps what that session's
  adapter profile allows.
- **`loop`** — drives a task ledger (`until: all_tasks_complete`, plus a `prompt:`
  each dispatched task session gets). One mechanically-verified session per `ready`
  task; `concurrency: N` runs up to `N` tasks from the current batch at once (default
  `1`, sequential). See the [ledger schema](design/spec-ledger.md) for what a task looks
  like — it's written by an earlier `prompt` node as a `kind: task-ledger` artifact,
  or by hand while you're still designing the workflow.
- **`check`** — automatic verification against data the engine already has: `builtin:
  baseline_compare` (did a passing suite start failing), `builtin: coverage_gate`
  (threshold), `builtin: findings_gate` (fails above a declared `max_severity`). No
  session opens; these read state the engine already derived.
- **`gate`** — a human decision. `assignee:`, `options: [...]`, and `on: {option:
  target}` to re-route on a choice exactly like `on_failure.goto` does. Renders
  through the console when attended, or as the same structured escalation object via
  `yunta mcp` / `yunta resolve-gate` when it isn't — no surface-specific logic. `external: { kind: pull_request, artifacts: [...], branch: "..." }` turns it
  into a forge round-trip instead: the listed paths (relative to `run.dir`) get
  committed to `branch` and opened as a PR for review there — the same relative paths
  the run's own worktree used, so a reviewer sees exactly what the run produced.
- **`parallel`** — a named group of child nodes run at once, with `join: all` (any
  child failing fails the group) or `join: any` (first success wins, the rest are
  interrupted). Children needing to compare notes mid-flight (not just after `join`)
  set `coordination: blackboard` on the group — see [MCP per-run tools](#mcp) below.
- **`executor`** — the extension point for anything `bash`'s bare exit code and
  `check`'s closed builtin list don't cover: `executor: <name>` (declared in
  `skills.executors:`) gets `with:` as JSON on stdin and returns a verdict as JSON on
  stdout.
- **`workflow`** — runs another workflow as a full, independent sub-run (`use:
  <name>`, `inputs: {...}`). `isolation: worktree` (default) gives it its own tree;
  `isolation: inherit` shares the parent's for tightly related phases, and siblings
  doing that must declare disjoint `scope`.

## `depends_on` and re-routing

`depends_on: [a, b]` means "wait for `a` and `b` to reach a terminal state." A node
with none and no `on_failure.goto` pointing at it runs as soon as the DAG lets it
(the toplevel, or whatever it depends on).

`on_failure: { goto: <id>, max_reroutes: N }` on a failing node redirects control to
`<id>` instead of failing the run. Once `<id>`'s own subgraph completes, control
returns and the original node runs again. `max_reroutes` bounds how many times this
happens before the run pauses on a gate instead — a correction loop is not allowed to
retry forever unattended.

## Scope and permissions

`scope: [globs]` on a `prompt` or `loop` node is a hard post-check boundary: after
the session closes, the engine diffs what actually changed against those globs. An
edit outside `scope` fails the node — not a warning, not something the agent can talk
its way past. That diff is the *only* thing enforcing scope: no adapter today blocks
an out-of-scope write while it happens, so an agent can write outside `scope` and the
node fails afterward for it.

`permissions: read-only | edit | full` is a separate, coarser ceiling, mapped onto the
adapter's own session profile: it governs which *tools* the agent may use at all — a
`read-only` node is handed no editing tools — not which files `scope` allows.

Config-level `permissions:` (in `.yunta/config.yaml`, and any org/user layer above
it) works the other way from every other config key: layers only ever *narrow*, never
re-widen, what's above them. A repo can't un-deny a command pattern its org layer
denied.

## Config layers and state

Yunta merges its configuration from three layers, key by key, most specific
winning:

- **repo** — `.yunta/config.yaml` in the project. Committed with the code; the
  place a team's own runners and defaults live.
- **user** — `config.yaml` under the user state root (`$YUNTA_HOME` when set,
  otherwise `~/.yunta`). Per-person overrides that never belong in the repo.
- **org** — `/etc/yunta/config.yaml`, or the path `YUNTA_ORG_CONFIG` names. A
  shared baseline an administrator sets once for every project on a machine.

Repo beats user beats org for every key — with the single deliberate exception
of `permissions:` above, which layers only ever narrow. The org layer is also
where shared *knowledge* comes from, distributed as packs rather than as this
one file — see [Knowledge layers](#knowledge-layers).

Two environment variables move all of this:

- **`YUNTA_HOME`** relocates the user state root as a whole — the user config
  layer *and* every byte of execution state (runs, worktrees, the event-log
  database) live under it together. Unset, it is `~/.yunta`. Point it at a
  scratch directory to give a CI or otherwise ephemeral environment its own
  isolated state. The roots a run freezes at creation are absolute, so
  `status`, `resume` and `gc` find that run from any directory — as long as
  `YUNTA_HOME` points at the state root it was created under.
- **`YUNTA_ORG_CONFIG`** overrides where the org layer is read from, for a
  machine that keeps its shared baseline somewhere other than
  `/etc/yunta/config.yaml`.

## Context

`context:` on a `prompt` or `loop` node assembles what that session sees, beyond the
prompt text itself: `files: [globs]`, `command: "<cmd>"` (stdout), `artifact: {node,
name}` (another node's declared output — this also creates the implicit dependency
edge, no separate `depends_on` needed), `mcp: {server, query}`, `run-events: {filter}`
(a read-only query into this run's own log), `ledger: {}` (the task ledger's current
state), `knowledge: {layers: [...]}` (repo/user-scoped project knowledge — see
[knowledge layers](#knowledge-layers) below), and `node-output: {node}` (a prior
node's own captured output, e.g. what a `parallel` group's blackboard consolidated
into after `join`).

Every source materializes what it actually saw — not just a hash of it — so
reconstructing a past session's context never needs to re-run the command, re-call
the MCP server, or re-read a file outside what was captured at the time.

### Artifacts the engine reads

Most artifacts are opaque: the engine records that the file exists and what it
hashes to, and its structure is whatever the session decided. `kind:` says the
opposite — that the engine parses the file, validates it, and turns its contents
into events. There are three: `task-ledger`, `findings` and `questions`.

Declaring a `kind:` is all it takes to have its shape published to whoever must
write the file. A node with `produces: [{ name: plan.yaml, kind: task-ledger }]`
opens its session with the shape already in context, annotated field by field,
and with the absolute path the engine will verify — the session's working
directory is the worktree, not the run directory, so it has no way to guess
that. Nothing else to declare, and an opaque artifact mounts nothing because it
has no shape to demand.

The same shape is available anywhere else you need it:

```
yunta schema                    # the kinds
yunta schema task-ledger        # the shape to write
yunta schema findings --json    # JSON Schema, for an editor to validate against
```

and through the `document_shape` tool on `yunta mcp`, so an agent connected to
the control plane finds it without anyone passing the format along.

When a file still comes back unreadable, the node fails with every problem in it
named at once — by task and field, never by a parser's path into the document —
and a node that declares several interpreted artifacts gets each file reported
under its own path. The node then gets one repair session against
`limits.max_artifact_repairs` (default 1): the node's own runner, the shape it
already mounts, those problems, and nothing to do but rewrite the declared files —
the malformed file is still on disk for the session to read and correct. Every node
that resolves a runner has this, whatever its kind, so a `loop` node's ledger is
repaired like a `prompt` node's plan. A node with no runner behind it — `bash`,
`check`, `gate` — has no repair cycle: there is no agent to instruct, and running
a command again is a retry, not a repair. Verification does not soften either way:
the node still fails if the repair does not land. What a rewrite cannot fix — an
artifact never produced, an empty one, one past `limits.max_artifact_bytes`, one
the filesystem refuses — fails straight away.

### Knowledge layers

`knowledge: { layers: [repo, user, org] }` resolves with local precedence: on a
filename conflict, repo beats user beats org. Omit `layers` to pull every layer.
The `org` layer is knowledge distributed as packs — the union of every installed
pack whose `pack.yaml` declares `contents.knowledge`, read straight from the
vendored `.yunta/packs/`. Between org packs there is no precedence: two installed
packs shipping the same filename is a resolution-time error naming both — shadow
the file with the repo's own copy, or remove one of the packs. An org layer with
no knowledge packs installed simply contributes nothing, same as a user layer
with an empty `~/.yunta/knowledge/`.

## Hooks

`hooks: { before: [...], after: [...] }` on a node (or `node_defaults.hooks` for the
whole workflow) runs shell steps immediately before or after the node's own work,
each as `{ run: "<cmd>", timeout_seconds?, on_failure: fail|warn }`. A failed
`before` hook stops the node before any session opens; `after` hooks run before
verification, so their own edits are subject to the node's `scope` like anything
else. Hooks never re-route — that's `on_failure.goto`'s job, not a hook's.

## Modes

`modes:` is an open, ordered map — `quick: {include: [...]}`, `standard: {...}`,
`full: { include: all }`, or any names you choose. `--mode <name>` on `yunta run`
selects one; promotion only ever moves forward through declaration order (a run
already in `standard` can't drop back to `quick`). A node marked `invariant: true`
must appear in every declared mode regardless of name or count — a mode narrows how
much deliberation happens, never how much verification does.

`include:` only ever names top-level node ids. A `parallel` group is atomic from a
mode's point of view — it's included or excluded whole, never by naming one of its
children; naming a child directly is a `check` error, not a way to reach inside the
group.

## Gates from the outside

A paused run doesn't need anything watching it: `yunta status <run_id>` shows what
it's waiting on and the exact option ids available, `yunta resolve-gate <run_id>
<option>` answers it from a completely separate process (or `yunta mcp`'s
`resolve_gate` tool, for an agent doing it programmatically), and the run picks the
decision up on its own next resume. Nothing about answering a gate requires the
process that hit it to still be alive.

The live view `yunta run` draws changes nothing about that. It reads the run; it is
never part of it. A gate waits on the event log, so one raised by a run whose view is
gone — piped, detached, or in a terminal that closed — is answered exactly the
same way, from anywhere.

## The Verified Work Receipt

`yunta receipt <run_id>` closes a finished run out as a certificate: markdown for a
PR, JSON for tooling, both derived entirely from the event log — criteria with exit
codes, baseline regressions, scope, which runners reviewed (and whether that was a
fan-out of independent ones), cost and CPTV, re-routes, and the event chain's own
integrity. Nothing in it is agent-written prose; every line traces back to a specific
event kind. It refuses a run that hasn't reached a terminal state yet — there's no
metrics to certify until `run_finished` lands.

Both formats are written to the run's own directory (`receipt.md`, `receipt.json`)
alongside `manifest.yaml` and `progress.md`, so a later `bash` node can pick them up —
for example, a closing `pr` node doing `gh pr create --body-file receipt.md` to make
the receipt the PR description itself, no copy-paste required.

## MCP

Two distinct surfaces, both stdio/HTTP MCP, neither a daemon:

- **Control plane** (`yunta mcp`): `list_workflows`, `run_workflow`,
  `workflow_status`, `resume_run`, `resolve_gate` — for an outer agent (e.g. Claude
  Code itself) driving Yunta as a tool. `run_workflow` always returns immediately; the
  run keeps going independent of the MCP session that started it.
- **Per-run tools**: a loopback HTTP MCP endpoint opened for the duration of a single
  agent session that declared `run_tools` capability — `yunta_post_finding`,
  `yunta_task_status`, `yunta_request_scope_expansion` for every such session, plus
  `yunta_get_blackboard` for a `coordination: blackboard` parallel group's own
  children (scoped to that group, and only visible after `join` — never mid-flight
  cross-talk that would anchor the group's judgments on each other).

## Authoring patterns

Two patterns worth adopting from the first workflow you write for real, not
retrofitting once wall-clock or noisy criteria become a problem.

### Criteria granularity

A ledger task's `criteria` (see the [ledger schema](design/spec-ledger.md#21-criteria)) run
red-before-green: the pre-check proves the criterion *can* fail before the task
starts. Keep each task's own criteria narrow and cheap — the specific test or check
that task's change is supposed to flip, not the whole suite. Re-running the entire
test suite on every single task in a loop is both slow (multiplied by every task in
the ledger) and a weak signal (a broad suite failing doesn't say *what* broke).

For the suite-wide, no-regression concern, use a `type: guard` criterion — checked
before and after, never counted as the thing this task proves — sparingly, on the
tasks where it matters, or once at the workflow's close via a `kind: check` node
(`builtin: baseline_compare`) shared by every task in the ledger instead of repeated
per task. `lint-fix.yaml` in the quickstart is this pattern in miniature: `lint`
verifies the whole workspace once, not per file changed.

### Shared build caches across worktrees

Isolation-by-worktree (the engine's default) means every run — and, under
`concurrency > 1`, every task within a run — gets its own working tree from a fresh
`git worktree`. For compiled languages this makes tree preparation the dominant cost
of wall-clock: a cold build in every worktree, every time.

The fix is a cache shared across worktrees, not skipping isolation. `yunta init`
detects the ecosystem and can propose a starting point; the general shape for any
build tool with a configurable output/cache directory is a `hooks.before` step that
points the worktree at a cache location outside it:

```yaml
node_defaults:
  hooks:
    before:
      - run: "mkdir -p ~/.yunta/build-cache/{{project.name}}/target && ln -sfn ~/.yunta/build-cache/{{project.name}}/target target"
```

The cache directory lives outside any single worktree (so it survives worktree
cleanup) but stays scoped to the project (so two projects' builds never collide).
The same shape works for a package manager's own cache (`node_modules`, Cargo's
registry cache, pip's wheel cache) wherever the tool supports pointing at an external
location — link it in a `before` hook, and every worktree that follows sees a warm
cache instead of starting cold.

## Packs

Covered on its own page: [packs.md](packs.md) — installing one, and creating
and publishing your own.

## Where the rest lives

This guide covers what's needed to write and reason about a workflow. See
[the documentation index](README.md) for adapters, troubleshooting, and the
compatibility policy. The full normative schema — every field, every
validation rule `yunta check` enforces, the event log's exact payloads, and
the rationale behind design choices — lives in the design corpus under
[`docs/design/`](design/README.md).
