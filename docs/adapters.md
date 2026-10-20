# Adapters

An adapter is the integration with one coding-agent CLI. Yunta ships three:

| Adapter | What it does | Requires |
|---|---|---|
| `claude-code` | Spawns the `claude` CLI headless and streams its output into the engine. | `claude` on `PATH`, authenticated. |
| `codex` | Spawns the `codex` CLI headless the same way. | `codex` on `PATH`, authenticated. |
| `mock` | Replays a scripted YAML fixture — no process spawned, no network, no LLM. | Nothing. |

A workflow node never names one of these directly — it names a **role**
(`runner: reviewer`), and your config resolves that role to a real adapter
binding. See [concepts](concepts.md#adapters-and-runners-who-actually-runs-a-session)
for why, and the [workflow guide](guide.md) for the node-level side of it.

## Configuring `runners:`

In `.yunta/config.yaml`:

```yaml
runners:
  executor:
    - { adapter: claude-code, model: claude-sonnet-4-6 }
  reviewer:
    - { adapter: codex, model: gpt-5-codex }
```

A role can list more than one candidate — the engine picks the first whose
adapter is healthy, so a role degrades gracefully across a team with mixed
tooling instead of hard-failing.

`yunta run --adapter <id>` pins the run to one adapter: each role resolves to
its candidate on that adapter, and the log records the candidates it passes
over. The pin is frozen with the run, so a resume, a detached child, a
successor and a child workflow all take the same one. A role with no candidate
on the pinned adapter refuses the run before it is created, naming the node and
the role. An adapter this binary does not build refuses it the same way, and the
refusal lists the adapters the config names that this binary builds.

A candidate can say how long its model reasons with `effort:` — `low`,
`medium`, `high` or `xhigh`, the levels both CLIs accept — and a node can say
its own, which wins. `claude-code` receives it as `--effort` and `codex` as
`-c model_reasoning_effort`, a resumed session included; absent, each CLI
reasons as it would on its own. Each session's level is on the log, in
`agent_session_opened`.

```yaml
runners:
  planner:
    - { adapter: claude-code, model: claude-opus-4-8, effort: high }
    - { adapter: codex, model: gpt-5-codex, effort: high }
```

## Agents

`agent:` on a candidate, or on the node itself (which wins), names an agent:
instructions the session works under, added to its CLI's own. The definition
is Yunta's, so the same agent runs on every adapter — a Markdown file at
`.yunta/agents/<name>.md` in the project, or one a pack ships and a workflow
names as `publisher/agent`:

```markdown
---
name: security-auditor
description: Reviews a change for security issues.
---
Read the diff before the code around it. Report each issue as a finding
with the file and line it is on.
```

The front matter holds `name`, which matches the file, and `description`, for
people; the body is the instructions. The model is the runner's, because a
model belongs to one provider, and the tools are the permission profile's —
a definition that names either is refused. `claude-code` hands the
instructions to the CLI with `--append-system-prompt-file`, and `codex` with
`-c developer_instructions=…`, on its command line, where `ps` lists them to
anyone who can list the machine's processes, so keep secrets out of a
definition. An agent that resolves to no definition refuses the run before it
is created, naming where it looked.

A run freezes each definition it uses when it is created. Yunta reads the
definition from the working tree the run is created in, committed or not: under
`isolation: worktree` the run's own checkout may lack an uncommitted definition,
and the run still gets it. Every session that follows gets the text the run was
born with, even if the file changes while the run is parked or running.

To move an agent from `.claude/agents/<name>.md`, copy it to
`.yunta/agents/<name>.md` and drop every front matter key but `name` and
`description`: its `model` goes on the runner, and its `tools` follow from
the node's `permissions:`.

## What a session may write

A node declares a `scope:`, and every session runs behind a **fence**: the
globs its tools may write under the worktree. The run's own directories sit
beside it: the artifact directory is writable in every session, and the shared
directories in every session that is not `read-only`. One function decides
whether a tool's write is inside the fence, and every adapter asks that same
function — what differs is how much of the fence each CLI can be made to
enforce, and each session's opening says which:

| Adapter | How it fences | What it covers |
|---|---|---|
| `claude-code` | A `PreToolUse` hook on every writing tool runs `yunta rail`, which asks the run's engine before the write happens, and refuses the write, with the cause, when the engine cannot be asked; commands stay inside the CLI's sandbox. | Edits exactly; commands by directory — the worktree, the run's directories and the temporary directories under `edit` with the `workspace-write` sandbox (the default); the run's directories and the temporary directories under `read-only`, and under `edit` with the `read-only` sandbox; anything under `full`, and under `edit` with `danger-full-access`. |
| `codex` | A `PreToolUse` hook on `apply_patch` runs `yunta rail`, which asks the run's engine about every file the patch names, and refuses the patch, with the cause, when the engine cannot be asked; commands stay inside the process's sandbox. | Edits exactly; commands by directory — the worktree, the run's directories and the temporary directories under `edit` with the `workspace-write` sandbox (the default); the run's directories and the temporary directories under `read-only`, and under `edit` with the `read-only` sandbox; anything under `full`, and under `edit` with `danger-full-access`. |
| `mock` | The fixture's `dialect:` (`claude-code` or `codex`) decides what the mock declares, as that CLI's adapter does; each scripted effect asks the engine the way that CLI's hook does. | What that CLI's adapter covers. |

Every session also loads its person's own configuration of the CLI — settings,
extensions, MCP servers, hooks — unless its node says `cli_config: isolated`, which
leaves it with only what the run mounts (`--setting-sources ""` and
`--strict-mcp-config` for `claude-code`, `--ignore-user-config` and `--ignore-rules`
for `codex`). A tool of an MCP server the session loads that way writes without
asking the rail, and the files it writes are not formatted as they are written: the
close judges them like any other write. The trees a CLI loads that configuration from inside a checkout —
`.claude/**` and `.mcp.json` for `claude-code`, `.codex/**` and `.agents/**` for
`codex` — are never written by any session of any run. The instruction files,
`AGENTS.md` and `CLAUDE.md`, are written only when a person grants them. A link
committed at one of these paths holds what it points to inside the checkout to
the same rule, since the CLI reads through it. The
close judges what each session wrote against these trees and files, git-ignored
files included, so a write the fence let through is still refused. No CLI sandbox
covers a protected file, a pattern that covers part of a directory, or a glob
that names no directory of its own, such as `**/AGENTS.md` or `**/CLAUDE.md`:
the sandbox settings take only whole trees, `dir/**`. The rail judges those for
the file tools. The close finds them through git: a pattern that names its
directory, such as `config/secrets.yaml`, is looked for inside that directory even
when the project ignores it; a pattern that names no directory is not looked for
inside a directory the project ignores whole, such as a build output or a
dependency tree, since what lies there belongs to the build or the dependency. A
run never commits what git ignores. No session's commands write the run's git
register, the repository's common git directory, wherever it lies — under a temporary
directory too, which the sandbox otherwise leaves writable — so every commit on a run's
branch is the engine's, with its trailers.

A refusal reaches the model with the reason and the way out — ask for more
scope, or report the need as a finding — and reaches the log as
`write_refused`. The fence is what keeps a write outside the scope from
happening; the diff Yunta takes after the session is still the guarantee. A
write that reaches that diff is a violation: the node fails with the list of
paths. It is also filed as the engine finding `engine-fence-breach`, beside that
failure and against the adapter, when the session's recorded confinement leaves
the checkout to the fence alone: its commands are confined and write nothing of
the checkout, which is a `read-only` profile, or an `edit` profile under the
`read-only` sandbox. Only the fence could have stopped such a write. Under any
other confinement a command may write the checkout, so the violation is all that
is filed. The coverage a session reports does not decide this: a confined
session names the temporary directories among its roots, so its coverage is
never `exact`.

No adapter reports a denial of its CLI's sandbox, so none writes one to the log.
Claude returns a command its sandbox refuses as an ordinary failed tool result,
and Codex 0.157.0 emits no item for one. The engine's rail records each refusal
it judges as `write_refused`. The mock asks the rail about each scripted write,
as the CLI it stands for does; narrowed to a filesystem fence, it skips a
denied write and reports nothing, as a sandbox does.

After a write, when the project's `format:` covers what was written, the CLI
tells the engine, which formats it and tells the session to read the file
again when it changed. Before a session ends its turn, its CLI asks the engine
too. The engine judges
the work the way the attempt's close will — a task's criteria, guards and
scope, or a node's documents and scope — and when the close would not take it,
the session is sent back to it with what stands in the way, recorded as
`stop_refused`. It does so once for one state of the work, and never while the
close can grant or escalate a scope request the session made: the close decides
that request, and a task's request the project refuses is answered in the run tool's
reply, at once. A criterion that cannot run, or one declared impossible, does not
send the session back either: the close blocks its task on the first, and the
planner takes the second. A stop hook that cannot reach the engine lets the session
stop: the close still judges it.
Both CLIs wait for that answer as long as a run-tool call may take.
A write hook that cannot reach the engine refuses the write, and the refusal says
why; it records no `rail_failed`, because no engine is listening to record it. A
failure the engine hears of is recorded as `rail_failed` with one closed cause:
`decode` for a call that does not read, `unauthorized` for a call without the
session's token, `timeout` for an answer that misses its window,
`judgement_failed` for a stop whose judgement cannot run (a judgement the
session's own stop cuts short did not fail, so it records nothing),
`formatter_failed` for a formatter that does not start, `no_fence` for a write in a session with no fence
to judge it by, and `listener_died` when a listener stops serving. A 400 and a 401
are each recorded once per listener.

Whatever the adapter, the session's prompt reaches the CLI on its standard
input, never as a command-line argument, so it is not readable from the
process list; and the values of the secrets a config declares live only in
the CLI's own environment — the engine hands them over wrapped, prints them
as `[redacted]` in every diagnostic, and never writes them to the event log.

`adapters:` (a sibling of `runners:`) overrides settings per adapter name —
most commonly `binary:` when the CLI isn't called `claude` or `codex`, or
isn't on `PATH` under that name:

```yaml
adapters:
  claude-code:
    binary: /opt/claude/bin/claude
```

`adapter_settings:` under an adapter carries what has no portable expression —
model, agent, permissions and budget are typed fields a node or runner sets,
never settings. Each adapter reads its own, and `yunta doctor` (and every
`run`, before opening a session) refuses a key it does not read, naming the
keys it does. Both read `sandbox`: the mode the `edit` profile's commands run
under (`workspace-write` by default; `read-only` narrows it, and
`danger-full-access` widens it, which needs the session's network open). The
setting is one input to the resolution of what a session may write and reach,
and the session's log entry records its effect.

```yaml
adapters:
  codex:
    adapter_settings:
      sandbox: read-only
```

The three permission profiles map onto the same confinement on both CLIs. A
session's commands run in its CLI's own sandbox, enforced by the operating
system: under `read-only` they write nothing of the checkout, under `edit` they
write the checkout and the run's directories (under the `read-only` sandbox, only
the run's directories), and under `full` nothing confines them. Under every
profile the commands may also write the temporary directories, `$TMPDIR` and
`/tmp`. Codex applies a confined session's permissions through a
profile of its own, `yunta-session`, which that session names; a session that is
not confined runs under `--sandbox danger-full-access`, which has no profile.
Claude Code takes the same permissions through the `sandbox` setting of its
session, and an unconfined session has none. The Codex profile's keys were
measured on Codex 0.157.0, so `yunta doctor` reports an older Codex, or one whose
version it cannot read, as unhealthy. Whether a session's commands reach the
network — loopback included — is the node's `network:`, else the project's
`permissions.network.default`, else open, and only a session that writes its
worktree can have one. A session that writes nothing of the checkout — a
`read-only` one, or an `edit` one under the `read-only` sandbox — never has
network, and `check` refuses `network: true` on a prompt or loop node that writes
nothing of the checkout. A closed network keeps a test suite from listening on
`127.0.0.1`, so leave it open where one does. An unconfined session refuses a
closed network: `check` refuses one on a prompt or loop node whose session nothing
confines — a `full` node, or an `edit` node on an adapter whose `sandbox` is
`danger-full-access` — and the run refuses such a session when it opens. Both
refusals judge prompt and loop nodes only: a `bash` node opens no session, so
`check` judges neither network. The engine's own timeout and token budget bound
every session, whichever CLI runs it. The timeout counts time the host is awake:
a machine that sleeps spends none of it, and the run's log records the suspension.
After a suspension, no session opens until the host has been awake for two
minutes, so a session is not lost to a laptop that wakes briefly and sleeps
again.

A session the engine ends before its own end, on a cancel or at its budget, is
interrupted and then killed, with its whole process group. If the adapter's
`kill` fails, the engine cannot tell the session stopped, and its process group
may still run. The node then fails with `session_not_stopped` and runs no close:
nothing of its tree is judged, landed or committed, because the session may
still be writing it. The failure records the tokens the session reported, and a
run cancelled from outside records it too. The group stays in the run's
`scratch/engine.json` for as long as it runs, past the engine that kept it:
`yunta close` settles it once that engine is gone, and `yunta cancel` signals it
while the run is still open.

## `mock`: not a test helper, a first-class adapter

`mock` reproduces an agent session from a YAML fixture — a scripted sequence
of events and filesystem effects, with injectable failures and latency. It
exists so the entire engine — tasks, degradation, cancellation, resume,
parallelism — is testable without ever calling a real model. This is why
`cargo test --workspace` and CI never touch a real LLM: every engine test
runs against `mock`.

You'll use it directly too: `yunta run <workflow> --adapter mock --fixture
<path>` runs a workflow against a fixture instead of a real session, and `yunta test` (cases
under `.yunta/tests/`) always uses it — see the [workflow
guide](guide.md#authoring-patterns) for writing workflows, and
[packs](packs.md#testing-a-pack-before-sharing-it) for using it to test a
pack you're authoring.

A mock run keeps its fixture. `yunta run --adapter mock --fixture <path>` reads
the fixture when the run is created and freezes its text with the run, so
`yunta resume` plays the rest of the run on that fixture and never opens a real
CLI. A mock run written before runs froze their adapter does not resume:
`yunta resume` refuses it, naming the mock, before it takes the run's checkout
and before it opens a session. Such a run that paused before its first session
has no mock session on its log to tell it apart, so it resumes like a run
without `--adapter`, on the CLIs its config names.

The mock keeps its claims in the run's `mock-claims.json`: the scripts its
sessions have taken, and the number the next fresh session id takes. A resumed
run takes only the scripts its earlier sessions did not, and each session it
opens takes the next fresh id. A session continued from an earlier one keeps
that one's id.

A fixture's session can script `kill: refuse`. Its `kill` then fails, and the
session keeps running for as long as the script still plays; once the script
has ended, `kill` answers as it does for any session. That is how a session the
engine cannot stop is reproduced without a real CLI.

A fixture's session can end on the account's limit:
`outcome: { type: limited, resets_at: <Unix seconds> }`. It closes the way its
dialect's CLI closes such a session. Under `claude-code` that is the rejected
rate-limit window naming the reset, then the refused result. Under `codex` it
is the usage-limit failure, which names no reset, so a `codex` fixture that
names one is refused when it is read. That is how a run that waits for the
account's reset is reproduced without a real CLI.

## `yunta doctor`

```bash
yunta doctor
```

Runs the exact same health probe `yunta run`/`yunta resume` run before
spending anything — binary present, version compatible, auth valid — for
every adapter your `runners:` names, and reports every one of them instead of
stopping at the first failure. Each check is a row: `✓` when it holds, `▲` when
it is worth a look and stops nothing, `✗` when a run would stop on it — and
`doctor` exits non-zero only for a `✗`:

```
  ✓ claude-code  healthy (1.2.3)
  ✗ codex        unhealthy — `codex` not found on PATH
```

A config whose `runners:` names no adapter has nothing to probe, and `doctor`
says so with the runner to declare, on the adapter CLIs this machine answers for
and the model left for you to name:

```
  ✗ runners  none declared, and `lint-fix` needs one

  declare a runner in .yunta/config.yaml — this machine answers for `claude-code`:
      runners:
        implementer:
          - { adapter: claude-code, model: <model> }
      defaults:
        runner: implementer
```

That is a `✗` only when a workflow in the catalog has an agent node that would
stop on it; with none, it is a `▲`. The same holds for a forge whose
token variable is not set: `doctor` fails for it only when a workflow in the
catalog opens a pull request, since a gate published to the forge asks on the
console without one. `yunta init` ends with the same runner step, and `yunta
check` adds it under a workflow that names a runner the config lacks.

It also validates every installed pack's own `requires:` against your merged
config: a `runners:` entry the merged `runners:` doesn't define (or defines with zero
candidates), an `mcp_servers:` name nothing declares, and a `programs:`
binary missing from `PATH` are each reported with what to add, naming the
pack that needs it. None of this blocks anything by itself — a pack can be
installed and configured later, the same way an adapter that isn't set up
yet doesn't stop `yunta init`. Run `yunta doctor` after adding a pack, or
whenever a run fails in a way that looks like a missing binary or an
unresolved role.

### `--session`: opening one for real

A probe asks the CLI for its version. That tells you the binary is there,
answers and authenticates; it does not tell you a session opens, because
`--version` never touches the configuration a run writes the CLI. A CLI
that refuses that configuration says so on stderr and exits before its
first line — from the outside, a node that failed with no exit and no
tokens.

```bash
yunta doctor --session
```

opens the smallest run there is — one `kind: prompt` node that declares a
`questions` document, with run tools mounted, driven through the same
machinery a workflow is — once per *binding*: an adapter, a model and an agent that some runner names. The
binding and not the runner name, because a session exercises a binding:
two runners naming the same one are not two things to check, and one a
runner falls back to is checked too, since a run reaches it exactly when
the first is down.

```
claude-code/claude-sonnet-5 (executor, reviewer): ok — 812 tokens
codex/gpt-5-codex (planner fallback): session died — session `codex` exited with code 2 before any terminal event — url is not supported for stdio
```

The prompt asks the session to call `yunta_submit_questions` with
`{"document":{"questions":[]}}`. `ok` means the run recorded acceptance of
that document, finished the node and finished the run. A session that opens
but does not submit it reports `session opened, no questions document`;
a CLI that exits before completing the session reports `session died`.
The empty document requires no human answer.

It spends one prompt per binding, which is why it is opt-in. It runs in a
sandbox of its own — nothing of your tree is touched, and your
`baseline:` suite is never measured, because the question is whether a
session can deliver a document, not what the tree measures.

## The two MCP servers

Two different servers carry the name of this system, and they are not the
same thing:

- **The control plane**, `yunta mcp`, which you register in your own
  CLI's configuration, under whatever name you give it. It talks over
  stdio and offers `list_workflows`, `run_workflow`, `workflow_status`
  and the rest.
- **The per-run server**, which the engine mounts into each session
  itself and always calls `yunta-run`. It talks over streamable HTTP,
  lives as long as the run does, and carries the tools a node uses to
  post findings and submit documents.

When Codex receives that per-run endpoint, Yunta sets
`mcp_servers.yunta-run.default_tools_approval_mode="approve"` for the spawned
CLI so its run tools can be called. The bearer token is passed through the
process environment; the CLI arguments contain only the environment variable's
name. Claude Code allows the run tools through its `--allowedTools` list, and
the mock adapter calls the server directly.

A CLI merges both entries into one table by key, so the per-run server
carries a name of its own: registering the control plane as `yunta` — the
natural thing to call it — leaves both intact.

Each CLI hands the server's tools to its model under a name of its own:
Claude Code as `mcp__yunta-run__yunta_task`, Codex as
`mcp__yunta_run__yunta_task`. An adapter declares that rule as its
`tool_naming` capability, and every text the engine shows a session names a
run tool by it, so a session calls the name its CLI has on the first try
rather than the bare `yunta_task`, which reaches nothing.

The event log names a run tool by its canonical name, such as
`yunta_check_artifact`, whichever CLI called it. Each adapter reads a run tool
call back to that name, so a CLI-specific name such as
`mcp__yunta-run__yunta_check_artifact` never reaches the log, and neither does
the generic `mcp_tool_call` for a run tool. A log written earlier, with those
names, still reads.
