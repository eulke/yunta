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
tooling instead of hard-failing. `agent:` on a candidate (or on the node
itself, which wins) names a custom agent definition, for adapters that
support one (`custom_agents` capability) — an adapter that doesn't declares
this openly rather than silently ignoring the field.

## What a session may write

A node declares a `scope:`, and every session runs behind a **fence**: the
globs it may write under the worktree, plus the run's own directories, which
stay writable wherever the session sits. One function decides whether a path
is inside it, and every adapter asks that same function — what differs is how
much of the fence each CLI can be made to enforce, and each session's opening
says which:

| Adapter | How it fences | What it covers |
|---|---|---|
| `claude-code` | A `PreToolUse` hook on every writing tool runs `yunta fence`, which answers before the write happens. | Exact under `edit` and `read-only`; tool calls only under `full`, which also exposes a shell. |
| `codex` | The sandbox the process itself runs under, by directory. | The worktree and the run's directories — by directory, never by glob. |
| `mock` | The fixture says which of the three levels it builds, and every scripted effect goes through the same judge. | Whatever the fixture declares. |

A refusal reaches the model with the reason and the way out — ask for more
scope, or report the need as a finding — and reaches the log as
`write_refused`. The fence is what keeps a write outside the scope from
happening; the diff Yunta takes after the session is still the guarantee, and
a write that reaches that diff despite an exact fence is reported as a finding
against the adapter.

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
keys it does. `codex` reads `sandbox`: the `codex exec --sandbox` mode the
`edit` profile runs under (`workspace-write` by default; `read-only` narrows
it, `danger-full-access` widens it). `claude-code` reads none.

The three permission profiles map onto each CLI's own mechanism. On
`claude-code`, `read-only` allows the non-mutating tools, `edit` allows file
editing and nothing that reaches a shell or the network, and `full` leaves the
whole tool set available; on `codex`, they are the `read-only`,
`workspace-write` and `danger-full-access` sandbox modes. `budget.max_turns`
reaches `claude-code` as `--max-turns`; `codex exec` has no turn cap, so there
the engine's own timeout and token budget bound the session.

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
stopping at the first failure:

```
claude-code: healthy (1.2.3)
codex: unhealthy — `codex` not found on PATH
```

It also validates every installed pack's own `requires:` against your merged
config: a `runners:` entry the merged `runners:` doesn't define (or defines with zero
candidates), an `mcp_servers:` name nothing declares, and a `commands:`
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

opens the smallest run there is — one `kind: prompt` node, run tools
mounted, driven through the same machinery a workflow is — once per
*binding*: an adapter, a model and an agent that some runner names. The
binding and not the runner name, because a session exercises a binding:
two runners naming the same one are not two things to check, and one a
runner falls back to is checked too, since a run reaches it exactly when
the first is down.

```
claude-code/claude-sonnet-5 (executor, reviewer): ok — 812 tokens
codex/gpt-5-codex (planner fallback): session died — session `codex` exited with code 2 before any terminal event — url is not supported for stdio
```

It spends one prompt per binding, which is why it is opt-in. It runs in a
sandbox of its own — nothing of your tree is touched, and your
`baseline:` suite is never measured, because the question is whether a
session opens, not what the tree measures.

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

A CLI merges both entries into one table by key, so the per-run server
carries a name of its own: registering the control plane as `yunta` — the
natural thing to call it — leaves both intact.
