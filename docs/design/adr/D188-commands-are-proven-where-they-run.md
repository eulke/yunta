---
number: D188
title: "A command handed to the engine is proven where it runs; one that cannot run is neither red nor green; a failed loop continues from the work it left only when there is some"
status: accepted
revises: []
revised_by: []
---

# D188 — A command handed to the engine is proven where it runs; one that cannot run is neither red nor green; a failed loop continues from the work it left only when there is some

## Context

A planner on Codex wrote every criterion of its tasks document as
`… | rg …`. Codex puts its own `rg` on the `PATH` of its sessions. The
engine runs criteria under `sh` with the environment the run's process
inherited, and there was no `rg` there. Every criterion exited 127 on
every check. The pre-check read the 127 as "red before the work", and each
attempt spent a session that could not change it; the implementer said,
three times, that the commands passed in its own shell. The criterion cache
kept the 127 for the tree. A person's retry reopened the task in a fresh
unit, which discarded work that was already correct.

Nothing recorded what the engine's commands ran with, so nothing could say
why a command worked in one place and not in the other.

## Decision

1. **A command handed to the engine is proven where it runs, when it is
   handed over.** When a session submits a tasks document, the engine runs
   each task's criteria in a checkout of the run's tree, with the run's own
   shell and `PATH`. It refuses the document when a criterion cannot run,
   and, for a task with no `depends_on` (whose pre-check starts from exactly
   that tree), when a criterion already passes or a guard already fails. A
   task the run already finished, repeated unchanged, is not run again. The
   refusal quotes what the shell said and where it looked, and the planner
   fixes the document in the same session. The loop's pre-check later reads
   the same answers from the cache. The rules are published with the
   document's shape, so they reach the planner before it writes.
2. **A command that cannot run is its own outcome.** Exit 127 (not found),
   126 (not executable) and a command the engine stopped say nothing about
   the tree. Such a criterion blocks its task at the pre-check, ends the
   cycle when it appears after an attempt, and is never cached. Every
   surface that shows the exit code says what it means.
3. **The environment is a fact of the run.** `run_created` and each
   `run_resumed` record the shell and `PATH` the run's commands get.
   `yunta status` says when a wake runs them in another environment than
   the run was born in.
4. **A failed loop continues from the work it left, only when it left
   some.** When a task blocks, the work its last attempt left is committed
   on its unit's branch and named on the status change. The failure's menu
   offers `continue-work` only when such work exists; otherwise `retry`,
   which starts from scratch, is the only way back. Continuing puts the
   work into a fresh unit as uncommitted changes and judges it before any
   session: a task it closes is done without one. Work that no longer
   applies blocks the task again, with nothing left to continue from.

## Rationale

Describing the environment to an agent would be a copy that drifts, and
no list of available tools covers a shell dialect, a platform's flags or a
variable one shell sets and another does not. Running the command where it
will run answers all of those at once. It costs little, because the loop's
pre-check reuses the answers.

A 127 read as red turns an environment problem into a work problem, and
only work problems can be fixed by another session. Keeping them apart is
what stops the spend.

`continue-work` is a separate option because `retry` already means
"reuses nothing the failed one did", and offering it where there is no
work would promise something that cannot happen.

## Rejected alternatives

**Resolving the programs a criterion names against the `PATH` at
submission.** It needs a shell parser, and it still misses dialect,
flags and variables.

**Making `retry` continue from the work for loops.** It would give one
option two meanings depending on the node.

**Offering `continue-work` always and failing when there is no work.** A
menu should only list what can happen.
