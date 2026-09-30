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

- **`bash`** — `run: "<command>"`, or `run: { command: <name> }` to run one of the
  project's own commands (see [Project commands](#project-commands)). Exit code is
  the verdict; no session, no agent.
- **`prompt`** — `prompt: "<text>"` (or `prompt: { file: path/to/prompt.md }` for
  longer ones). Opens one agent session. `runner:` picks which role from `runners:`
  in config resolves it; `permissions: read-only|edit|full` caps what that session's
  adapter profile allows.
- **`loop`** — drives a tasks document (`until: all_tasks_complete`, plus a `prompt:`
  each dispatched task session gets). One mechanically-verified session per `ready`
  task; `concurrency: N` runs up to `N` tasks from the current batch at once (default
  `1`, sequential). See the [tasks schema](design/spec-tasks.md) for what a task looks
  like — an earlier `prompt` node produces it as a `kind: tasks` artifact, or
  you write one by hand while you're still designing the workflow. The loop works
  from whichever tasks document the run holds, however it came by one: produced
  by a node, given as a `type: document` input, mounted in from a parent, or
  inherited from the run this one succeeds.
- **`check`** — automatic verification against data the engine already has: `builtin:
  baseline_compare` (did a passing suite start failing), `builtin: coverage_gate`
  (threshold), `builtin: findings_gate` (fails above a declared `max_severity`). No
  session opens; these read state the engine already derived.
- **`gate`** — a human decision. `assignee:`, `options: [...]`, and `on: {option:
  target}` to re-route on a choice exactly like `on_failure.goto` does. Renders
  through the console when attended, or as the same structured escalation object via
  `yunta mcp` / `yunta resolve-gate` when it isn't — no surface-specific logic. `external: { kind: pull_request, artifacts: [...], branch: "..." }` turns it
  into a forge round-trip instead: `artifacts:` names artifacts of this run the same
  way `produces:` does — a kind, or an opaque file name — and the engine commits the
  bytes the run holds for each to `branch` and opens a PR for review there, under the
  name that artifact's identity gives it. A run that holds none of one fails the node
  instead of publishing a partial review.
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
  `isolation: none` shares the tree the node works in, for tightly related phases,
  and siblings doing that must declare disjoint `scope`.
- **`pull_request`** — pushes the run's own branch and opens a pull request of it into
  `project.base_branch` (or the branch the run started from), through the forge the
  project configures: `title:` and an optional `body:`, both templates. Running again
  pushes the same branch and finds the pull request it opened, so a rerun never opens
  a second one. It needs `forge.github` in the config — `yunta check` refuses it
  otherwise, unless the node is [optional](#optional-nodes) — and a run with a
  worktree of its own, since `isolation: none` gives the run no branch. `yunta run`
  refuses to start when the variable holding the forge's token is not set, and
  `yunta doctor` says whether the token reaches the repository and may push there.

```yaml
forge:
  github:
    repo: acme/web
    token_env: GITHUB_TOKEN
    remote: origin              # where the run's branch is pushed; `origin` when absent
```

## `depends_on` and re-routing

`depends_on: [a, b]` means "wait for `a` and `b` to reach a terminal state." A node
with none and no `on_failure.goto` pointing at it runs as soon as the DAG lets it
(the toplevel, or whatever it depends on).

`on_failure: { goto: <id>, max_reroutes: N }` on a failing node redirects control to
`<id>` instead of failing the run. Once `<id>`'s own subgraph completes, control
returns and the original node runs again. `max_reroutes` bounds how many times this
happens before the run pauses on a gate instead — a correction loop is not allowed to
retry forever unattended. A failure the run's frozen config causes — a key it
leaves unset — is never re-routed: the correction reads the same config, so the
failure goes straight to the run's `defaults.on_failure`.

## Scope and permissions

`scope: [globs]` on a `prompt` or `loop` node is a hard post-check boundary: after
the session closes, the engine diffs what actually changed against those globs. An
edit outside `scope` fails the node — not a warning, not something the agent can talk
its way past. That diff is the *only* thing enforcing scope: no adapter today blocks
an out-of-scope write while it happens, so an agent can write outside `scope` and the
node fails afterward for it.

`scope: run` declares a node's reach without naming a path: it may change what the
run has changed since its base, as that stands when the node starts. It is the scope
for a node that corrects the run's own work — the fix a failing lint re-routes to —
in a repository whose layout the workflow never saw, which is why a pack reaches for
it. The engine resolves the paths when the attempt starts and records them on its
`node_started`, so its close is audited against what it was given. A write elsewhere
fails the node like any scope violation, and a person may grant it. A node scoped to
a run that has changed nothing may change nothing.

A node that declares no `scope:` works in the run's own tree, and what it leaves
there is committed on the run's branch when it closes — finished or failed — so what a
later node pushes is what the run did, and removing the run's worktree loses nothing.
Two such nodes running at once are committed together by whichever closes last, and a
`parallel` group commits what its children left when it closes. What you edit in the
run's tree while it is paused is committed when the next node starts, as found there,
so that node and everything after it builds on your edit. What git ignores is
never committed; anything else a node writes is, so keep build output and secrets in
`.gitignore`. A run with `isolation: none` works in your own checkout and commits
nothing: what its nodes write stays uncommitted for you. The engine commits without
running your git hooks.

`permissions: read-only | edit | full` is a separate, coarser ceiling, mapped onto the
adapter's own session profile: it governs which *tools* the agent may use at all — a
`read-only` node is handed no editing tools — not which files `scope` allows.

Config-level `permissions:` (in `.yunta/config.yaml`, and any org/user layer above
it) works the other way from every other config key: layers only ever *narrow*, never
re-widen, what's above them. A repo can't un-deny a command pattern its org layer
denied.

`permissions.paths.deny` names what no run may write, whatever a workflow or a pack
declares — the project's CI, the configuration of its own checks:

```yaml
permissions:
  paths:
    deny: [".github/**", "eslint.config.*"]
```

Every layer's denies stand together. A session's write fence refuses those paths
where its adapter enforces one; a node with a checkout of its own that wrote one
fails without landing, and a task that wrote one is not integrated. A failure over a
denied path never offers `grant`, a scope-expansion request for one is refused
without asking anyone, and a node's request for one is never put to a person. The
list is empty unless the project writes one, so no workflow fails for lacking it.

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

## Project commands

A workflow says *what* it needs run; the project says *how*. `commands:` in the
config names what this project runs for each capability:

```yaml
commands:
  lint: "pnpm lint"
  typecheck: "pnpm typecheck"
```

and a `bash` node or a hook step runs one by name:

```yaml
name: checked
nodes:
  - id: lint
    kind: bash
    run: { command: lint }
```

The name is the workflow's and the text the project's, so a workflow — a pack's
above all — never has to know which tool a repository lints with. A project's
command runs exactly as the project wrote it: no `{{...}}` template is rendered
into it, and `permissions.commands` governs it like any other command.
`commands:` merges key by key across config layers, so a repo can name its own
`lint` and keep the `test` its user or org layer declares.

A node that names a command the config does not declare cannot run, and no
attempt of the run can change that — the run's config is frozen when it is
created. `yunta check` refuses such a workflow before the first token, naming the
node and the command, and a run that meets one anyway fails the node without
offering a retry or spending its `on_failure` re-route.

`yunta init` writes `commands:` for what it finds in the repository: the
scripts a `package.json` declares for `lint`, `typecheck`, `test`, `format` and
`build`, run by the package manager its lockfile names (`pnpm lint`,
`npm run lint`, `npm test`); the usual tools of a Rust, Go or Python project
otherwise. It also writes the suite a run measures (`baseline.suite`) and, when
`origin` is on GitHub, a `forge:` for it. Detection only proposes: nothing reads
a detected value but the config a person commits. Where a workflow names a
command the config lacks and the repository answers for it, `yunta check`,
`yunta doctor` and `yunta pack add` say what to declare:

```text
node `lint`: the project declares no command `lint` — a run is refused until it does
  detected here: declare `commands: { lint: "pnpm lint" }`
```

## Context

`context:` on a `prompt` or `loop` node assembles what that session sees, beyond the
prompt text itself: `files: [paths]` (literal paths from the top of the run's tree;
an entry written `{ path: <path>, optional: true }` is one the node can do without —
when it's missing the session reads a marker in its place instead of the node
failing), `command: "<cmd>"` (stdout), `artifact: {node,
kind}` or `artifact: {node, name}` (another node's declared output, named the way
that node declares it — this also creates the implicit dependency edge, no
separate `depends_on` needed), `mcp: {server, query}`, `run-events: {filter}`
(a read-only query into this run's own log — the whole of it, or only its
`failed` nodes, its `findings`, or its `deviations`: the departures from the
plan task sessions declared and the answers they got), `tasks: {}` (the tasks document's current
state), `knowledge: {layers: [...]}` (repo/user-scoped project knowledge — see
[knowledge layers](#knowledge-layers) below), and `node-output: {node}` (a prior
node's own captured output, e.g. what a `parallel` group's blackboard consolidated
into after `join`).

Every source materializes what it actually saw — not just a hash of it — so
reconstructing a past session's context never needs to re-run the command, re-call
the MCP server, or re-read a file outside what was captured at the time.

### Artifacts the engine reads

A node declares what it produces as a list of bare strings —
`produces: [tasks, notes.md]`.
`tasks`, `findings` and `questions` name the documents a node produces and the
engine reads, validates and turns into events. `answers` names a fourth the
engine writes itself, when a person replies to a `questions` document; a node
cannot declare it, and a node that follows the one that asked reads it with
`context: [{ artifact: { node: <the node that asked>, kind: answers } }]`. Every
other string is the name of a file the engine only carries: it records that the
file exists and what it hashes to, and its structure is whatever the session
decided. Those four names are therefore not available as file names, and
`yunta check` says so when a reference spells one as a `name:`.

A node produces at most one document of each kind, so the kind is the whole
identity: `(node, kind)` is what the run answers by, and declaring the same kind
twice is a check error because there is no second one. Nothing names a file —
the engine writes the view itself, at `artifacts/<node>/<kind>.yaml`. A fan-out
that runs the same node once per runner needs no template for that: each sibling
is a node of its own and holds its own document.

The node declares, the engine publishes the shape and names the tool that takes
the document, the session hands the document over, and the engine takes it into
the run. A node with `produces: [tasks]` opens its session with the shape already in
context — annotated field by field, followed by the rules the document has to
satisfy — and with a `yunta_submit_tasks` run tool whose one argument,
`document`, is that same schema. A `questions` artifact arrives the same way,
through `yunta_submit_questions`. No session writes an interpreted file itself.
Nothing else to declare, and an opaque artifact mounts nothing because it has no
shape to demand.

The tool answers in the same call, with the verdict the node's close reaches: the
engine reads the object into the same type and runs the same rules. An acceptance
reports what the engine understood — `tasks.yaml — accepted. 6 task(s)
registered: ...` — and puts the canonical document into the run: the bytes under
`objects/`, the acceptance on the log. A refusal lists every
rule the document breaks, all at once — or, when the
object does not read into its kind at all, that one problem and the path where it
sits (`tasks[1].scope`), because a value of the wrong type stops the read
before any rule can hold. Either way the session fixes it and submits again: a
refused document costs a call, not a session. The document the node holds is the
last one it got accepted, and the node's close asks the log for it — no file
stands in for one that never arrived.

Findings are reported one at a time instead. A session calls `yunta_post_finding`
the moment it sees one — validated on its own, so a refusal names what to fix in
that finding and everything already reported stands. `yunta_update_finding` replaces
one by id with its whole new content, and `yunta_withdraw_finding` takes one back
with a reason; a withdrawal is final, and a finding that comes back is a new id. A
`prompt` or `loop` node that declares `produces: [findings]` gets that document
derived at its close, from every finding it reported that still stands,
in the order it first reported them — a node that reports nothing gets a document
with an empty list. A finding outlives the session that found it, so a session
that dies after reporting loses nothing.

A document nobody submits fails the node, named by the node that owes it and the
document it owes rather than by a file — there was never going to be one — and
there is no second session to instruct. A file a command node declared and never
wrote fails the node too, named by the path the close went looking at, as does one
that is empty, past `limits.max_artifact_bytes`, or refused by the filesystem. An adapter that
mounts no run tools fails a node that declares an interpreted artifact before the
session is dispatched — the document has no way in.

A `bash`, `check`, `gate` or `executor` node can declare an interpreted artifact
too. It writes the file itself, under `{{node.artifacts}}`, and the close reads it
with the same code and holds it to the same rules. When such a file does not read
back, the node fails with every
rule problem in it named at once — by task and field, in the document's own words —
and a node that declares several interpreted artifacts gets each file reported under
its own path.

An opaque artifact is a file its session writes: a node that declares one gets a
directory of its own added to what its session may write, and can call
`yunta_check_artifact` to confirm the file is there before the session ends. That
directory is `{{node.artifacts}}` in the node's own templates — which is how a
`bash`, `check` or `executor` node names it too. It belongs to that node alone, so
two nodes that declare the same name never write over each other. The directory
belongs to the session rather than to the attempt: a node picking a session back
up under `on_interrupt: resume_session` keeps what that session wrote there,
because it is work that session did, and every other attempt — a command node, or
a fresh session replacing an interrupted one — opens on an empty directory, so no
earlier attempt's file closes this one as work it never did. A node that declares
only interpreted artifacts is granted nothing outside its worktree.
`yunta_check_artifact` also reads back the document the run already holds for that
node, so a session can see its meaning survived the parse.

The same shape is available anywhere else you need it:

```
yunta schema                    # the kinds
yunta schema tasks              # the shape of the document
yunta schema findings --json    # JSON Schema, for an editor to validate against
```

and through the `document_shape` tool on `yunta mcp`, so an agent connected to
the control plane finds it without anyone passing the format along.

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
each as `{ run: "<cmd>", timeout_seconds?, on_failure: fail|warn }` — `run` takes a
project command by name here too. A failed
`before` hook stops the node before any session opens; `after` hooks run before
verification, so their own edits are subject to the node's `scope` like anything
else. Hooks never re-route — that's `on_failure.goto`'s job, not a hook's.

## Modes

`modes:` is an open, ordered map — `quick: {include: [...]}`, `standard: {...}`,
`full: { include: all }`, or any names you choose. `--mode <name>` on `yunta run`
selects one; promotion only ever moves forward through declaration order (a run
already in `standard` can't drop back to `quick`). A successor starts with its
predecessor's tasks, the ones already done still done — it picks the work up
where that run left it rather than repeating what is already in the tree. A node marked `invariant: true`
must appear in every declared mode regardless of name or count — a mode narrows how
much deliberation happens, never how much verification does.

An invariant that runs a command on the tree — a `bash` node, or a `baseline_compare`
or `coverage_gate` check — verifies the tree it left, not the run forever. When a
later node changes the tree (a `fix-findings` after the checks, a corrective node, a
person between two attempts), the run runs that invariant again before it starts
anything else, asks a gate or finishes. A workflow does not need to repeat `lint`
and `tests` after every node that edits code; it declares them once, as invariants.
An invariant is declared on a top-level node: modes name a `parallel` group rather
than its children, and a child only ever runs with its group, so `yunta check`
refuses `invariant: true` on a child and says to move it out.

The run freezes that declaration order in its manifest. For a new run with
`quick`, `standard`, then `full`, a promotion from `standard` can select `full`.
Older manifests remain readable; if an older writer saved their modes in a
different order, the original order cannot be recovered from that file alone.

`include:` only ever names top-level node ids. A `parallel` group is atomic from a
mode's point of view — it's included or excluded whole, never by naming one of its
children; naming a child directly is a `check` error, not a way to reach inside the
group.

### Optional nodes

`optional: true` on a top-level node says the run can do without it when the project
lacks what it needs — a [project command](#project-commands) the config does not
declare, or a forge. Such a node is left out of the run the way a mode leaves a node
out: it never runs, and what waits on it waits on what it waited on. A node that only
the optional one leads to — the re-route that fixes what a failing lint reports — goes
with it. Where the project does declare what the node needs, it runs like any other.

```yaml
name: linted
nodes:
  - { id: build, kind: bash, run: "true" }
  - id: lint
    kind: bash
    run: { command: lint }
    optional: true
    invariant: true
    depends_on: [build]
    on_failure: { goto: fix-lint, max_reroutes: 1 }
  - id: fix-lint
    kind: prompt
    runner: mechanical
    prompt: "Fix exclusively the errors in the report."
    context: [{ node-output: { node: lint } }]
```

The run decides this once, when it is created, and records it: `yunta status` shows
the node as `skipped — not in this run: the project declares no command `lint``, and
counts it apart from the work (`· 1 left out`). A node that is not optional and needs
something the project lacks is refused before the run instead. An `invariant` may be
optional: no mode drops it, but a project may have nothing to run for it. `yunta
check` refuses `optional` on a child of a `parallel` group, and on a node another
node the run keeps re-routes to or reads from.

## Reviewing what a gate approves

A gate can put what it asks about in front of you. `shows:` names artifacts the
way a `context:` source does, and the gate waits for them:

```yaml
name: reviewed-plan
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Break the work into tasks."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    options: [approve, adjust]
    on: { adjust: plan }
    shows: [{ node: plan, kind: tasks }]
```

At the decision you read the plan the way it is reviewed: what it changes and why,
the shapes it creates, its risks, then task by task what each does, what it touches
and what proves it done — and the path to `tasks.md`, the whole plan in Markdown
with its diagrams and a graph of the order its tasks run in. The log records the
exact version you saw by its hash, so an approval is an approval of those bytes.
A gate after the work that shows the same plan — before a pull request, say —
reads first every departure from it a person accepted while its tasks were built:
what the plan said, what was built instead and why, and what they said accepting
it.

A plan a gate shows has to say those things. Next to `tasks:` it carries a
`summary`, a Markdown `description` (code blocks and `mermaid` diagrams welcome), a
`design` with the types, interfaces or schemas it creates or changes, and `risks`
and `out_of_scope` when there are any; every task has its own `description` and
every criterion says what it `proves`. The engine refuses a plan it will show
without the summary, the descriptions and the `proves`, and tells the planner what
is missing in the same answer.

An option that sends the run back to a node with a session, like `adjust` above,
asks what should change and doesn't take an empty answer: those words are what the
planner picks its work back up with, in the same session, instead of planning again
from the brief. From another process, say them with `yunta resolve-gate <run_id>
adjust --text "…"`.

To skip the review altogether, run a mode that leaves the gate out: fragua's
`quick` goes from the plan straight to the work.

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

- **Control plane** (`yunta mcp`): `document_shape`, `list_workflows`,
  `run_workflow`, `workflow_status`, `resume_run`, `resolve_gate`,
  `answer_questions` — for an outer agent (e.g. Claude Code itself) driving Yunta as
  a tool. `run_workflow` always returns immediately; the run keeps going independent
  of the MCP session that started it.
- **Per-run tools**: a loopback HTTP MCP endpoint opened for the duration of a single
  agent session that declared `run_tools` capability — `yunta_post_finding`,
  `yunta_update_finding`, `yunta_withdraw_finding`, `yunta_check_artifact` and
  `yunta_task_status` for every such session; `yunta_task`, `yunta_check_task`,
  `yunta_request_scope_expansion` and `yunta_declare_deviation` for a loop's task
  sessions, which read their task and the plan it belongs to, judge their work, and
  say where it departs from the plan through them — a departure keeps the task open
  until a person accepts it or sends it back (a loop therefore needs a runner that
  can hold these tools); `yunta_check_scope` and `yunta_request_scope_expansion` for
  the session of a node that declares `scope:`, which audits its work against that
  scope and asks a person to widen it rather than writing outside it;
  a `yunta_submit_<kind>` tool for each submittable kind the node declares under
  `artifacts.produces` (see [artifacts the engine reads](#artifacts-the-engine-reads));
  and `yunta_get_blackboard` for a `coordination: blackboard` parallel group's own
  children (scoped to that group, and only visible after `join` — never mid-flight
  cross-talk that would anchor the group's judgments on each other).

## Authoring patterns

Two patterns worth adopting from the first workflow you write for real, not
retrofitting once wall-clock or noisy criteria become a problem.

### Criteria granularity

A task's `criteria` (see the [tasks schema](design/spec-tasks.md#21-criteria)) run
red-before-green: the pre-check proves the criterion *can* fail before the task
starts. Keep each task's own criteria narrow and cheap — the specific test or check
that task's change is supposed to flip. A narrow criterion says *what* the task did;
a broad suite failing doesn't.

The suite-wide, no-regression concern is the run's, not the planner's. When your
config declares `baseline.suite` and the suite passed when the run measured it,
every task of a loop is held to that suite as a `guard`: its pre-check, the checks
its session runs through `yunta_check_task`, its close and its integration all run
it, and `yunta_task` lists it among the task's guards with what it is there to show.
A change that breaks what passed keeps the task that made it open, while its session
can still answer for it — instead of surfacing after every task closed, at a
`baseline_compare` node nobody who made the change is left to fix. Don't repeat the
suite in a task's criteria; a task that declares it is judged by its own
declaration. A suite that was already red when measured holds no task to it.

The price is the suite's duration per check. Each task works in its own checkout,
so a build tool that keeps its output inside the tree builds from scratch there
once per task; a shared directory (below) keeps later builds warm. A
`baseline_compare` node at the workflow's close still earns its place: it covers
what nodes after the loop change.

A criterion runs under `sh` with the run's own `PATH`, not in the shell of the
agent that wrote it — an agent's CLI can put tools on its own `PATH` (a bundled
`rg`, say) that the engine's commands never see. When a planner submits a tasks
document, the engine runs every criterion there before accepting it and refuses one
that cannot run, quoting what the shell said. A criterion that calls a file the task
itself will create checks for it first, so it fails before the work instead of not
running at all:

```yaml
tasks:
  - id: verify-script
    title: Add the verification script
    scope: ["scripts/verify.sh"]
    criteria:
      - cmd: "test -f scripts/verify.sh && sh scripts/verify.sh"
```

### Shared build caches across worktrees

Isolation-by-worktree (the engine's default) means every run — and, under
`concurrency > 1`, every task within a run — gets its own working tree from a fresh
`git worktree`. For compiled languages this makes tree preparation the dominant cost
of wall-clock: a cold build in every worktree, every time.

The fix is a cache shared across worktrees, not skipping isolation. `shared_dirs:`
names directories every command and every session of a run shares, each under the
variable it is exported as:

```yaml
shared_dirs:
  CARGO_TARGET_DIR: ~/.cache/yunta/target/my-project
```

Every command the run spawns — bash nodes, hooks, criteria, the suite — sees the
variable, and so does every agent session. The engine creates the directory when the
run wakes, and every session that may write keeps it writable inside its sandbox, so
the build an agent runs and the one the engine checks it with land in the same
cache. A read-only session is given nothing to write. The path is written in full or
from `~`, never relative to a checkout, and the variable is never `PATH`.

The same shape works for any tool that reads its output or cache directory from a
variable, such as pip's `PIP_CACHE_DIR` or Go's `GOCACHE`. Keep the directory
scoped to the project, so two projects' builds never collide.

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
