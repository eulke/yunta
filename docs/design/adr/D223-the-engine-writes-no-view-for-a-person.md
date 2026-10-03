---
number: D223
title: "The engine writes no view for a person; a document is drawn where it is read"
status: accepted
revises: [D16, D195]
revised_by: []
---

# D223 — The engine writes no view for a person; a document is drawn where it is read

## Context

The engine wrote two files only a person reads. `progress.md` was rewritten after
every node from the log, and was described as part of every node's context —
but no prompt ever mounted it, no command named it, and it was written only when
a node closed, so it went stale whenever a gate was answered, a run cancelled or
closed. `tasks.md` was written beside a plan when it was accepted, as the whole
plan in Markdown for the person reviewing it.

Both were presentation done by the executor of runs. Both were second copies of
something else: of the log, of the plan's bytes. And neither could say what a
reader of them needed later — `tasks.md` could not show the spec's tests on the
tasks they hold, since the spec is accepted after the plan.

## Decision

1. **The engine writes no file for a person to read.** It records the log and the
   artifacts; it hands data — a receipt, the plan as the run judges it, the
   documents a decision shows — to whoever shows them.
2. **`progress.md` is retired.** Where a run stands is `yunta status`, derived from
   the log when it is read; what a node's session reads of the run comes through
   the engine's tools, never a copy in its prompt.
3. **`tasks.md` is retired.** A plan is read whole with
   `yunta status <run> --node <planner>`, drawn on read as the run judges it —
   with the spec's tests on their tasks — and a forge that publishes a plan draws
   the same document as Markdown.
4. **A plan still explains itself** (D195): its summary, description, design,
   decisions and what each criterion proves are in the document the engine runs,
   and are required when a person will read it.

## Rationale

A file written for a person is a promise that it is current, and a copy can only
be current as of the last time it was written. Drawing a document when it is read
keeps one source — the log, the artifacts — and lets each medium draw it its own
way. A run's directory still holds every artifact as the engine reads it, so
nothing a person needs is lost: what is gone is a second, staler rendering of it.

## Rejected alternatives

**Rewrite the views on every event that changes them.** It keeps the files current
at the cost of presentation inside every engine write, and a third medium would
add a third file.

**Keep `progress.md` as a twin of `yunta status`.** Nothing reads it; a twin that
nobody opens is a copy to keep in step for its own sake.
