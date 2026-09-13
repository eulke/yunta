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

## A node failed on an artifact it declared

```
artifacts/plan/tasks.yaml: 2 errors
  task `graph-cmd`: `scope` is empty; every task declares at least one glob, the only paths it may touch
  task `graph-cmd`: `depends_on` names `t9`, which no task in this file declares
```

A node that declares a kind under `artifacts.produces` — `tasks`, `findings` or
`questions` — has to leave behind a document the engine can read. The heading names the file and how many
problems it has; each line below names one problem and the entry it belongs to, in
the document's own words. `yunta schema <kind>` prints the shape the document is
read against.

For a `prompt` or `loop` node, the document arrives through a run tool and the
engine takes it into the run. A session that never handed one over closes the
node on a failure of its own, headed by the node rather than by a file:

```
node `plan`: 1 error
  handed over no tasks document — produce it before the node ends, or stop declaring it here
```

No path is named because none was ever going to be there: such a document is
never a file on its way in, and a file left in the node's directory is not one
either, because no acceptance explains it. The run's log carries every submission
the session made, accepted or refused, under `artifact_submitted`. There is no
second session: a node that produces nothing fails once, and the failure is not
retryable.

For a `bash`, `check`, `gate` or `executor` node, the command writes the file
itself. Check that it writes the name the node declared, under
`{{node.artifacts}}` — the node's own directory, which the engine empties at the
start of every attempt.

A file that was never written, is empty, is past `limits.max_artifact_bytes`, or
that the filesystem refuses is reported as a failure of the file rather than of the
document, and names which of those it is.

## The engine refused a document or a finding a session offered

```
The tasks document `tasks.yaml` was not accepted. Fix these and submit again:

  1. task `graph-cmd`: `scope` is empty; every task declares at least one glob, the only paths it may touch

  2. task `graph-cmd`: `depends_on` names `t9`, which no task in this file declares
```

This is the engine answering `yunta_submit_tasks`, `yunta_submit_questions`,
`yunta_post_finding`, `yunta_update_finding` or `yunta_withdraw_finding` inside the
session, with the verdict the node's close reaches. It is not a failure: the
session reads the numbered list, fixes exactly those problems, and calls the tool
again. A correction costs one call, not a session, and there is no limit on how
many times a session tries.

A document that reads into its kind is refused with every rule it breaks, all at
once. A document that does not read into its kind is refused with that one problem
and the path where it sits:

```
  1. does not parse at `tasks[1].manual_review`: invalid type: string "yes", expected a boolean
```

A value of the wrong type stops the read, and the rules only hold over a document
that parsed, so fixing the structure and submitting again is what surfaces them. A
refused finding leaves every other finding the node reported standing; only the one
in that call is rejected.

The refusal also lands in the run's log — `artifact_submitted` with a `refused`
outcome, or `finding_refused`, each carrying the whole report — so how often a run
gets a document wrong is a fact about the run and not something only the session
saw.

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

## `resume` says the run is broken because of an artifact

```
run is broken: run `01J...` no longer holds the bytes its log accepted for 1
of the 3 artifact(s) it names: `artifacts/plan/tasks.yaml`: object
`a1b2...` holds content that hashes to `c3d4...` — the bytes under
`objects/` are not the bytes the run accepted
```

Waking a run reads back every artifact its log accepted, because the log
names bytes by their hash and the run keeps them under `objects/`. That
message means one of those objects is gone, or its content no longer hashes
to its own name — somebody edited or replaced a file under `objects/`, or
the filesystem lost part of it. The run stops before doing any further work:
its own history says it holds something it can no longer hand to a node.

What to do:

- **Put the bytes back.** If the run's directory came from a backup or a
  copy, restore `objects/` from it — the object's name *is* its sha256, so
  any copy of the right content is the right object, wherever it comes from.
- **Editing an artifact is not how you change one.** The file under
  `artifacts/` is a view the engine writes and never reads; deleting it is
  harmless, and editing it changes nothing. `objects/` is the run's
  evidence, and nothing outside the engine writes there.
- **If the bytes are gone for good**, the run cannot be resumed — its
  artifacts are part of what it is. Start a new run from the same inputs.

`yunta verify <run_id>` reports the same check on demand, without resuming.

A run created by a Yunta older than the object store reports instead that it
holds artifacts this binary cannot verify — a `minor` finding, not a break.
That log names files rather than objects; see [compatibility](compatibility.md).

## `resume` says the run is broken because of its worktree

```
run is broken: run `01J...` works in `/home/me/.yunta/worktrees/01J...`, whose
HEAD `9f1c...` no longer has the run's base commit `4a77...` behind it: every
task, scope check and criterion this run's log records was established against
a tree that this one is not a continuation of. Put it back on the run's own
branch (`git -C /home/me/.yunta/worktrees/01J... checkout yunta/01J...`), or
on any commit that still descends from `4a77...` — ...
```

Waking a run asks its worktree two questions, and this is the second one
failing. The run branched from a commit, and everything on its log — each
task marked done, each `scope_checked`, each green criterion — was
established against a tree descending from it. A `git reset --hard` behind
the run's own commits, a rebase, or a checkout of an unrelated branch takes
that commit out of the tree's history, and from then on the state derived
from the log describes a tree that is not there. The run stops before doing
any more work on it.

What to do:

- **Put the tree back.** `git -C <worktree> reflog` shows every commit that
  tree has been on, including where the run left it. `git -C <worktree>
  checkout yunta/<run_id>` returns it to the run's own branch; any commit
  that still descends from the base commit works.
- **Under `isolation: none`** the run works directly on your checkout, so a
  `git pull --rebase` or a rebase while the run was paused is the usual way
  this happens. The remedy is the same — the reflog, then back onto a commit
  that still descends from the base.
- **If that history is gone for good**, this run's is too: start a new run
  against the tree as it is now. The work in the tree is not lost by this —
  only the run's claim to have verified it.

**What is *not* a problem: a changed tree.** New commits on top, or
uncommitted edits you made by hand during the pause, are the expected case
and the run resumes over them without a word. The worktree is the work; its
content is never verified against a snapshot. What answers a changed tree is
the criteria memo, which is keyed on a hash of the tree, so the run re-runs
its criteria against what is there instead of trusting a result about a tree
that is gone.

## `resume` cannot find the run's worktree

```
the run's branch `yunta/01J...` has no worktree at
`/home/me/.yunta/worktrees/01J...` (there is nothing at that path) — the
run's history and artifacts are intact; bring the checkout back with `git
worktree add /home/me/.yunta/worktrees/01J... yunta/01J...`, run from the
repository the run was created in, and resume again
```

The first of the two questions. The run is not broken: its event log and the
objects under `objects/` — everything that is its evidence — are untouched,
and git still holds its branch with every commit the run made. Only the
checkout is missing, and the command in the message brings it back exactly.
Run it from the repository the run was created against (the one whose
`.git/worktrees/` holds the entry), then resume.

If the run has `isolation: none` the message is different — it says the
directory is not a git working tree at all. That run works on the checkout it
was created in rather than on one of its own, so resume it from there.

## Something looks corrupted, or a replay disagrees with what you remember

```bash
yunta verify <run_id>
```

Checks a run's two mechanical guarantees and reports them apart:

- **the event chain** — every link recomputed from the log as persisted, so
  an altered payload or a deleted, inserted or reordered event is named with
  the seq it begins at;
- **the objects** — every artifact the log accepted, read back and hashed
  against its own name.

The two are independent: a corrupt object leaves the chain intact, and an
altered event leaves the objects alone. Either one failing exits non-zero.
This is the mechanical way to confirm (or rule out) tampering or corruption —
never guess from `status` output alone if you suspect this.

## Still stuck

`yunta <command> --help` is the source of truth for flags — this doc and the
[workflow guide](guide.md) cover behavior, not every flag. If a run's
behavior doesn't match anything here, `yunta status <run_id>` and the run's
own `progress.md` (in the run's directory) are both derived straight from
the event log and are the most reliable place to start.
