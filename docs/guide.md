# Workflow guide

The [README quickstart](../README.md#quickstart-a-three-node-workflow-from-scratch)
gets a `bash` → `prompt` → `bash` chain running. This is the reference for
everything past that: the other node kinds, context, permissions, gates, modes, and
two authoring patterns worth using from the start — criteria granularity and shared
build caches across worktrees.

Nothing here needs a pack installed. Packs (`yunta pack add`, on the roadmap) are a
distribution mechanism for sharing workflows and knowledge across projects — every
mechanism below works the same in a single, pack-free repo.

## Node kinds

Every node has an `id`, an optional `depends_on: [ids]`, and one `kind`:

- **`bash`** — `run: "<command>"`. Exit code is the verdict; no session, no agent.
- **`prompt`** — `prompt: "<text>"` (or `prompt: { file: path/to/prompt.md }` for
  longer ones). Opens one agent session. `runner:` picks which role from `runners:`
  in config resolves it; `permissions: read-only|edit|full` caps what that session's
  adapter profile allows.
- **`loop`** — drives a task ledger (`until: all_tasks_complete`, plus a `prompt:`
  each dispatched task session gets). One mechanically-verified session per `ready`
  task; `concurrency: N` runs up to `N` tasks from the current batch at once (default
  `1`, sequential). See the [ledger schema](spec-ledger.md) for what a task looks
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
its way past. `permissions: read-only | edit | full` is the corresponding *pre-check*
ceiling, mapped onto the adapter's own session profile so the agent's tools are
constrained before it ever gets a chance to write outside scope.

Config-level `permissions:` (in `.yunta/config.yaml`, and any org/user layer above
it) works the other way from every other config key: layers only ever *narrow*, never
re-widen, what's above them. A repo can't un-deny a command pattern its org layer
denied.

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

A ledger task's `criteria` (see the [ledger schema](spec-ledger.md#21-criteria)) run
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

A pack is a distributable, versioned bundle of workflows, skills,
knowledge and docs — never something that extends the engine itself, only content it
already knows how to run. A pack **declares** the roles it needs (a role name plus a
permissions ceiling, e.g. `reviewer` at `read-only`) and its own permissions ceiling
(`declares:` in `pack.yaml`) — never a concrete adapter, model or secret; the
installing team resolves those roles against its own `runners:`, so the same pack
runs unedited on a team that's all Claude Code and one that's all Codex.

This repo ships two example packs at [`packs/`](../packs/) — `yunta/starter`
(two minimal workflows: a one-node `fix` and a fan-out `review`) and
`yunta/fragua` (the full reference pipeline: grill, a verified task ledger,
lint→fix, a baseline check, multi-runner review, PR). Both install and remove
like any third-party pack; the engine treats them no differently.

```bash
yunta pack add github.com/acme/review-pack@v1.2.0
yunta pack list
yunta pack update acme/review-pack v1.3.0
yunta pack remove acme/review-pack
```

`add` clones the ref, vendors it to `.yunta/packs/<publisher>/<name>/` (checked into
the repo alongside the team's own code — a pack's contents are versioned with your
history, not fetched fresh on every checkout), and records `{ref, commit, content
hash}` in `.yunta/yunta.lock`. Nothing updates itself: `update` always names an exact
target ref. `list` re-hashes what's actually vendored on disk against the lock and
says so if they've drifted — an offline, no-network way to confirm the vendoring
hasn't been tampered with or gone stale.

Once installed, a pack's workflows and skills are addressable by
`publisher/name` — the name after the slash is the workflow or skill's own file
basename (as declared in `pack.yaml`'s `contents:`), not the pack's own `name`
field, so a publisher's installed packs share one flat namespace:

```bash
yunta run acme/review
yunta check acme/review
```

and the same form works inside a workflow (`use: acme/qa-review`) and a node's
`skills:` list (`skills: [acme/review-rubric]`). Resolution always tries the
repo's own `.yunta/workflows/` first — a repo file at the same
`publisher/name` path always wins over the pack: a local workflow with the
same name shadows the one from the pack. `yunta list` reflects the same
two-layer view and the same shadowing.

Composition is scoped to a pack's own contents: a workflow shipped inside a
pack may freely `use:` another workflow from the *same* pack, but reaching
into a different pack, or back out to the repo, is rejected by `check` —
cross-pack composition is out of scope for v1.

A pack is code from someone else, and it can be audited by reading it: `yunta
pack audit acme/review-pack` prints a full static inventory of every workflow
it ships — every `bash`/hook/loop command, every context source and exactly
what it points at, permissions and required agent per node, `mcp` servers
reached, executors flagged as code, and each workflow's **complete, untrimmed
prompt text**. It's inventory, never a verdict: nothing here flags
content as "suspicious" — that would be trivially evadible and would only
give false confidence. It also reports whether the pack ships its own tests
under `.yunta/tests/` (same format `yunta test` uses) and whether they pass.
`add` runs the same audit automatically, before anything is vendored —
nothing lands in your repo unseen.

`declares:` in `pack.yaml` is a ceiling, not a description, and `yunta check`
enforces it as one: a `prompt`/`loop` node inside a pack can never request a
session permission above what that pack's manifest promises — including a
node that declares no `permissions:` of its own, which still falls back to
the engine's own `edit` default and can exceed a `read-only` ceiling just the
same. Exceeding it fails `check` with an error naming the node, the pack and
both the declared and requested level; the same rule follows composition, so
a child workflow reached through `use:` from inside the pack is checked
against that pack's ceiling too. Declaring any `executors:` raises the bar
further, and how far is the installing team's own call:
`permissions.packs.executors` in the config (an org-ceiling setting — lower
layers only narrow it, never re-widen it) decides. `prompt`, the default, refuses to
install or update a pack that ships executable code unless `--yes` confirms
it, after the audit has shown exactly what the executors are; `deny` refuses
outright — no flag overrides a permissions ceiling; `allow` installs without
asking. `permissions.packs.publishers.allow`, when non-empty, additionally
restricts which publishers can be installed or updated at all — a refusal
names the config layer that declares the restriction. `update` enforces both
the same way `add` does: a new ref is where new executor code first appears.

`requires:` is the mirror image of `declares:` — a floor the *installing*
team's own config must clear, not a ceiling the pack promises. `yunta doctor`
validates every installed pack's `requires:` against the merged config
alongside its usual adapter health check: a `roles:` entry that `runners:`
doesn't define (or defines with zero candidates), an `mcp_servers:` name
nothing declares, and a `commands:` binary that isn't on `PATH` are each
reported with what to add, naming the pack. Nothing here blocks `pack add` or
`check` — a pack can be installed and configured for later, same as an
adapter that isn't set up yet doesn't stop `yunta init`.

Starting a run from a pack's own workflow freezes exactly which pack version
produced it — publisher, name, the pack's own semver, and the exact commit
`yunta.lock` recorded, all in the run's manifest from the moment it's
created. `yunta pack update` afterward changes nothing about a run already
in flight: `resume` only ever re-reads that manifest, never the vendored
pack on disk again, the same immutability every other frozen field
(`workflow`, prompts, config) already has.

## Where the rest lives

This guide covers what's needed to write and reason about a workflow. The full
normative schema — every field, every validation rule `yunta check` enforces, the
event log's exact payloads, and the rationale behind design choices — lives in the
project's Notion workspace (the Contrato del Run, the Adapter trait spec, and
Decisiones y racionales). `docs/` in this repo mirrors the subset that tracks
day-to-day implementation status: [`spec-ledger.md`](spec-ledger.md) (the task ledger
artifact this guide's `loop` section points at) and [`eventos.md`](eventos.md) (event
payloads, for anyone building tooling against the log directly).
