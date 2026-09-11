# Concepts

The pieces, and how they fit, before you write a real workflow or configure a
team's setup. If you just want to run something, the [README
quickstart](../README.md#quickstart-a-three-node-workflow-from-scratch) gets a
workflow going faster — come back here when you want to know *why* it behaves
the way it does.

## A run is a workflow plus a log, nothing else

A **workflow** is a YAML file: nodes, dependencies, re-routes. It describes
what *could* happen — it isn't itself a record of anything happening.

A **run** is what you get from `yunta run <workflow>`: a **manifest** (the
workflow plus config, inputs and resolved runners — hashed and frozen the
moment the run is created) and an **event log** in a local SQLite file.
Nothing about a run's state lives anywhere else. `yunta status`, `yunta
resume` and `yunta stats` all derive their answer by replaying that log from
the start — that's what makes killing the engine mid-run and resuming safe:
there's no in-memory state to lose, only events to replay.

This is also why a run is immutable once frozen: change the workflow file
after starting a run, and that run keeps executing the version it started
with. Want the new version? Start a new run.

## Nodes execute; verification decides

Every node is one of a fixed set of **kinds** (`bash`, `prompt`, `loop`,
`check`, `gate`, `parallel`, `executor`, `workflow` — see the [workflow
guide](guide.md#node-kinds)). Only `prompt` and `loop` open an agent session;
everything else is either a shell command, a decision, or composition.

The engine never takes an agent's word for whether something worked. A
node's **criteria** — a `bash` node's exit code, a `check` node's builtin, an
`executor`'s own exit code — are what decide `done` vs `failed`. This is the
one rule everything else in Yunta serves: an agent can propose a fix, but it
cannot mark its own work verified.

## Adapters and runners: who actually runs a session

An **adapter** is the integration with one coding-agent CLI — `claude-code`,
`codex`, or `mock` (a scripted fixture-driven adapter with no LLM behind it,
built for tests and CI). All three speak the same internal contract, so a
workflow never names an adapter directly inside a node.

Instead, a node names a **role** (`runner: reviewer`), and a project's
`.yunta/config.yaml` resolves each role to one or more candidate bindings
under `runners:` — `{ adapter, model, agent? }`. This indirection is what
lets the same workflow run unedited on a team that's all Claude Code and one
that's all Codex: the workflow author picks roles, the installing team picks
adapters. See [adapters](adapters.md) for configuring this and for what
`mock` is actually for.

## The engine tells agents what it expects

Three artifact kinds are parsed and validated rather than just stored:
`task-ledger`, `findings` and `questions`. They are strict — a key that is not
in the schema fails the node that produced it — which only works because the
schema is published to whoever has to write one, never assumed.

A node that declares one of those kinds gets its shape in context automatically.
Outside a run, `yunta schema <kind>` prints it and the `document_shape` tool on
`yunta mcp` returns it, so an agent working in your repo can look the format up
the same way it looks up anything else. Nobody has to relay a format by hand.

When a document still comes back wrong, the failure names every problem in it in
the document's own terms — "task `t1`, criterion 1: expected a mapping" rather
than a path into a parser — and the engine gives the session one chance to write
it again with those problems in hand. That budget is
`limits.max_artifact_repairs`, and the verification itself never relaxes.

## Packs: sharing workflows without extending the engine

A **pack** is a distributable, versioned bundle of workflows, skills,
knowledge and docs — plain content the engine already knows how to run,
never a way to add new capabilities to it. Packs are optional: every
mechanism in the [workflow guide](guide.md) works the same in a single,
pack-free repo.

A pack **declares** a permissions ceiling it promises never to exceed and
**requires** roles/commands/MCP servers the installing team's own config
must provide — it never assumes a concrete adapter, model or secret. See
[packs](packs.md) for installing one, and for creating and sharing your own.

## Where a run's state can pause, and how it resumes

A node can end in one of `finished | failed | skipped | waiting`. `waiting` means
a `gate` is asking a person for a decision — nothing is running underneath
it, and it survives the engine restarting exactly like any other state.
`yunta resolve-gate` (or the MCP `resolve_gate` tool) answers it from a
completely separate process; the run picks the decision up on its own next
resume. Nothing about a run depends on the process that started it, or hit
the gate, staying alive.

## Where to go next

- Writing a workflow: the [workflow guide](guide.md).
- Setting up adapters for your team, or understanding `mock`/`doctor`: [adapters](adapters.md).
- Installing, creating or publishing a pack: [packs](packs.md).
- Something isn't behaving as expected: [troubleshooting](troubleshooting.md).
- Versioning and what breaks between releases: [compatibility](compatibility.md).
