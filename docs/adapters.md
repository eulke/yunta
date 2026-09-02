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
config: a `roles:` entry `runners:` doesn't define (or defines with zero
candidates), an `mcp_servers:` name nothing declares, and a `commands:`
binary missing from `PATH` are each reported with what to add, naming the
pack that needs it. None of this blocks anything by itself — a pack can be
installed and configured later, the same way an adapter that isn't set up
yet doesn't stop `yunta init`. Run `yunta doctor` after adding a pack, or
whenever a run fails in a way that looks like a missing binary or an
unresolved role.
