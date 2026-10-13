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
`-c developer_instructions=…`. A node whose agent resolves to no definition
fails before its session opens, naming where it looked.

To move an agent from `.claude/agents/<name>.md`, copy it to
`.yunta/agents/<name>.md` and drop every front matter key but `name` and
`description`: its `model` goes on the runner, and its `tools` follow from
the node's `permissions:`.

## What a session may write

A node declares a `scope:`, and every session runs behind a **fence**: the
globs it may write under the worktree, plus the run's own directories, which
stay writable wherever the session sits. One function decides whether a path
is inside it, and every adapter asks that same function — what differs is how
much of the fence each CLI can be made to enforce, and each session's opening
says which:

| Adapter | How it fences | What it covers |
|---|---|---|
| `claude-code` | A `PreToolUse` hook on every writing tool runs `yunta rail`, which asks the run's engine before the write happens; commands stay inside the CLI's sandbox. | Edits exactly; commands by directory — the worktree and the run's directories under `edit`, only the run's directories under `read-only`, anything under `full`. |
| `codex` | A `PreToolUse` hook on `apply_patch` runs `yunta rail`, which asks the run's engine about every file the patch names; commands stay inside the process's sandbox. | Edits exactly; commands by directory — the worktree and the run's directories under `workspace-write`, nothing under `read-only`, anything under `full`. |
| `mock` | The fixture's `dialect:` (`claude-code` or `codex`) decides what the mock declares, as that CLI's adapter does; each scripted effect asks the engine the way that CLI's hook does. | What that CLI's adapter covers. |

Every session also loads its person's own configuration of the CLI — settings,
extensions, MCP servers, hooks — unless its node says `cli_config: isolated`, which
leaves it with only what the run mounts (`--setting-sources ""` and
`--strict-mcp-config` for `claude-code`, `--ignore-user-config` and `--ignore-rules`
for `codex`). The trees a CLI loads that configuration from inside a checkout —
`.claude/**` and `.mcp.json` for `claude-code`, `.codex/**` and `.agents/**` for
`codex` — are never written by any session of any run. The instruction files,
`AGENTS.md` and `CLAUDE.md`, are written only when a person grants them. The
close judges what each session wrote against these trees and files, git-ignored
files included, so a write the fence let through is still refused. The one place it
does not look is inside a directory the project ignores whole, such as a build
output or a dependency tree, for a pattern that names no directory of its own
(`**/AGENTS.md`): what lies there belongs to the build or the dependency. A run
never commits what git ignores.

A refusal reaches the model with the reason and the way out — ask for more
scope, or report the need as a finding — and reaches the log as
`write_refused`. The fence is what keeps a write outside the scope from
happening; the diff Yunta takes after the session is still the guarantee, and
a write that reaches that diff despite an exact fence is reported as a finding
against the adapter.

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
`stop_refused`. It does so once for one state of the work, never while the
attempt's end owes the session an answer about its scope, and a hook that
cannot reach the engine lets the session stop: the close still judges it.
Both CLIs wait for that answer as long as a run-tool call may take.

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
under (`workspace-write` by default; `read-only` narrows it,
`danger-full-access` widens it).

```yaml
adapters:
  codex:
    adapter_settings:
      sandbox: read-only
```

The three permission profiles map onto the same confinement on both CLIs. A
session's commands run in its CLI's own sandbox, enforced by the operating
system: under `read-only` they write nothing of the checkout, under `edit` they
write the checkout and the run's directories, and under `full` nothing confines
them. One difference is still open: under `read-only` with run directories, the
two CLIs confine commands differently, as the table above shows, until a later
change aligns them. Whether they reach the network — loopback included — is the
node's `network:`, else the project's `permissions.network.default`, else open; a
closed network keeps a test suite from listening on `127.0.0.1`, so leave it
open where one does. `check` refuses `network: false` on a `full` node, whose
commands nothing confines. The engine's own timeout and token budget bound every
session, whichever CLI runs it. The timeout counts time the host is awake: a
machine that sleeps spends none of it, and the run's log records the suspension.
After a suspension, no session opens until the host has been awake for two
minutes, so a session is not lost to a laptop that wakes briefly and sleeps
again.

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
