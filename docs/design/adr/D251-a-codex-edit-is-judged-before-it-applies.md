---
number: D251
title: "A Codex edit is judged before it applies"
status: accepted
revises: [D172]
revised_by: []
---

# D251 — A Codex edit is judged before it applies

## Context

D172 gave Codex the filesystem level of the fence: its sandbox confines
writes by directory, so an edit outside a session's scope but inside the
worktree landed and only the post-check caught it. Codex refusals reached
the log, when at all, as opaque sandbox denials of commands, never as the
path a session tried to write — so the evidence rule could never grant one.
A `read_only` session with roots to keep writable could not be built at all
(`SealedRoots`).

Codex now runs hooks headless. A `PreToolUse` hook sees every `apply_patch`
with the patch itself, and exit 2 with a reason refuses it; the model reads
the reason. A hook passed with `-c` runs only once trusted, and its trust is a
hash Codex records under the hook's key in `hooks.state`.

## Decision

1. **Codex declares `FenceLevel::ToolCalls`.** Its `PreToolUse` hook on
   `apply_patch` is `yunta rail codex before-write`, which asks the engine
   about every path the patch adds, changes, deletes or moves to.
2. **The hook is trusted by its own hash, in the same call.** The adapter
   passes the hook and its trust as `-c` overrides, the hash computed the way
   Codex computes it. No `--dangerously-bypass-hook-trust`: no hook the
   person has not trusted runs to make room for the run's.
3. **Commands stay inside the sandbox.** `read_only` without roots runs
   under `read-only`; a `read_only` session that must write its roots runs
   under `workspace-write`, the hook refusing every edit outside them; `edit`
   under the configured mode; `full` under `danger-full-access`. Coverage
   says what the commands were left: the workspace and the roots, nothing,
   or everything. `SealedRoots` goes.

## Rejected alternatives

**`--dangerously-bypass-hook-trust`.** It runs every enabled hook the person
never trusted, theirs or the checkout's, to run one of ours.

**Keep the filesystem level.** It leaves Codex edits unjudged and its
refusals pathless, a different run on each CLI.
