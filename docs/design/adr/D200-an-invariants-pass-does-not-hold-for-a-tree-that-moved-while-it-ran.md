---
number: D200
title: "An invariant's pass does not hold for a tree that moved while it ran"
status: accepted
revises: [D198]
revised_by: []
---

# D200 — An invariant's pass does not hold for a tree that moved while it ran

## Context

An invariant that runs a command on the tree verifies the tree it left, and
the scheduler runs it again when a later node leaves the run at another tree.
That rule reads the order of closes, and with `max_parallel_nodes` above one it
has a hole: `lint` can run beside a node that changes the tree. Whether `lint`
closes before or after that node, its own finish can name the changed tree —
it captured the tree after the other node wrote — while what `lint` actually
read was the tree half-changed. The pass then stood for a tree nobody verified.

A failure could also move the tree, and named none.

`invariant: true` on a child of a `parallel` group was accepted and honored by
nothing: modes name the group, not its children, and a child only runs with its
group, so it could never run again alone.

## Decision

1. **A close names the tree it left, finished or failed.**
2. **A close that moved the tree marks every attempt that ran beside it.** When
   a node closes leaving the run's tree different from the one it started on,
   every other attempt that was open while it ran, or closed after it started,
   and that did not itself start on the new tree, is marked as having had the
   tree move under it. A new attempt starts unmarked.
3. **A marked invariant's pass holds for no tree:** the scheduler runs it again,
   alone, before anything else starts, a gate is asked or the run finishes.
4. **`invariant: true` is declared on a top-level node.** `yunta check` refuses it
   on a child of a `parallel` group, naming the child and its group — a
   declaration the run could not honor is refused rather than ignored.

## Rationale

Overlap is what makes a verdict unreliable, and overlap is on the log: every
start and every close has a position. Marking from the log keeps the scheduler a
pure function of it. A re-run runs alone, so it marks nobody and nobody marks
it, and the rule ends.

## Rejected alternatives

**Forbidding invariants beside writers.** It would make `max_parallel_nodes`
depend on which nodes declare `invariant`, and a parallel run would stop
instead of running the check once more.

**A warning for an invariant inside a group.** The flag would stay inert while
reading as a promise; the file is the place to fix it.
