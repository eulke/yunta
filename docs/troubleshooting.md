# Troubleshooting

Common ways a workflow, a run, or a pack refuses to do what you expected,
and what each one actually means. Most of these are deliberate refusals, not
bugs — Yunta prefers a named, actionable error over guessing what you meant.

## `yunta check` refuses the workflow

`check` validates statically, before any session opens. The message always
names the node and the exact problem:

- **`node "x" references runner "y", which "runners:" does not define`** —
  add the role under `runners:` in `.yunta/config.yaml` (see
  [adapters](adapters.md#configuring-runners)), or fix the typo in the
  node's `runner:`.
- **`cycle in depends_on: ...`** — the path is printed; break the cycle,
  there's no partial-order fallback.
- **`node "x" on_failure.goto targets unknown node "y"`** / **`node "x"
  depends_on unknown node "y"`** — a typo'd or removed node id. Every
  `goto`/`depends_on` target must exist in the same workflow.
- **`node "x" references {{inputs.y}}, which inputs: does not declare`** —
  add `y` under the workflow's own `inputs:`, or fix the template reference.
- **a pack ceiling error naming the node, the pack, and both the declared
  and requested permission level** — a node inside a pack asked for more
  than that pack's `declares.permissions` promises. See
  [packs](packs.md#installing-and-using-a-pack) — the ceiling can't be
  exceeded from the consuming side; it's the pack's own manifest that has to
  change.
- **`yunta_schema: "..."` — ... (this binary speaks schema N)`** — the
  workflow (or an installed pack) declares a schema range your installed
  `yunta` binary doesn't satisfy. Check [compatibility](compatibility.md) for
  what's changed between schema versions.

## `yunta doctor` reports an adapter unhealthy

```
codex: unhealthy — `codex` not found on PATH
```

The diagnostic names the actual problem: binary missing, version
incompatible, or auth invalid. Fix that specific thing and re-run — `doctor`
runs the identical probe `yunta run` runs before spending anything, so a
healthy `doctor` means a run won't fail on setup for that adapter. See
[adapters](adapters.md#yunta-doctor).

If `doctor` reports a pack's `requires:` unmet (a role, an `mcp_servers:`
name, or a command not on `PATH`), it names the pack — add what's missing to
your own config, you don't need to touch the pack itself.

## A node failed with "scope violated: N file(s) outside the declared globs"

The session edited something outside the node's `scope:` globs. This is a
hard post-check, not a warning — the fix is either narrowing what the
session actually touches (tighten the prompt, or the `scope:` boundary was
too aggressive for what the task legitimately needs) or widening `scope:` if
the edit was legitimate. See [scope and permissions](guide.md#scope-and-permissions).

## A node's criteria never turn green

`yunta status <run_id>` shows which criterion is failing and its exit code.
If the same criterion keeps failing across every re-route
(`on_failure.goto`) up to `max_reroutes`, the run pauses on a gate instead of
looping forever — that's expected, not a hang. Widen what the correction
node is allowed to see (`context:`) or change (`scope:`) before assuming the
criterion itself is wrong.

## `isolation: none` refuses to start

```
`<path>` has uncommitted changes — isolation `none` requires a clean tree
```

`isolation: none` runs directly against your working tree instead of a
fresh `git worktree`, so it requires the tree to already be clean, and
refuses a second concurrent run against it. Commit or stash first, or switch
to the default `isolation: worktree` if you don't specifically need to run
in place.

## A run is stuck in `waiting`

Not stuck — paused on a `gate`, waiting for a human decision, and it
survives the engine restarting. `yunta status <run_id>` shows what it's
waiting on and the exact option ids; `yunta resolve-gate <run_id> <option>`
answers it from any process. See [gates from the
outside](guide.md#gates-from-the-outside).

## `yunta pack add`/`update` refuses

- **`publisher "x" is not in permissions.packs.publishers.allow`** — your
  config (the layer is named in the error) restricts which publishers can be
  installed. Add the publisher there, or get the pack from an allowed one.
- **`this pack declares N executor(s) ...`** — `permissions.packs.executors`
  is `prompt` (the default) and needs `--yes` after you've reviewed the
  printed audit, or is `deny` and refuses outright regardless of `--yes`.
  See [packs](packs.md#installing-and-using-a-pack).

## Something looks corrupted, or a replay disagrees with what you remember

```bash
yunta verify <run_id>
```

Recomputes the event log's hash chain end to end and reports exactly where
it breaks, if it does. This is the mechanical way to confirm (or rule out)
log tampering or corruption — never guess from `status` output alone if you
suspect this.

## Still stuck

`yunta <command> --help` is the source of truth for flags — this doc and the
[workflow guide](guide.md) cover behavior, not every flag. If a run's
behavior doesn't match anything here, `yunta status <run_id>` and the run's
own `progress.md` (in the run's directory) are both derived straight from
the event log and are the most reliable place to start.
