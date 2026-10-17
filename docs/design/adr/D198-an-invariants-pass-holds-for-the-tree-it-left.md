---
number: D198
title: "An invariant's pass holds for the tree it left"
status: revised
revises: []
revised_by: [D200]
---

# D198 — An invariant's pass holds for the tree it left

*(Revised by D200: a pass taken while another node moved the tree under the
invariant holds for no tree, and a failure names the tree it left too.)*

## Context

`invariant: true` said one thing: every mode includes the node. Where the node
sat in the graph decided what it verified. Fragua's `standard` mode runs `lint`
and `tests`, then a two-runner review, then `fix-findings`, which edits code,
then the `ship` gate and the PR. Nothing ran `lint` or `tests` after
`fix-findings`: the run asked a person to ship a tree no check had seen, and
the log still showed both checks as passed. A corrective node reached by a
re-route had the same effect on every check before the one that failed.

The log could not tell either case: it recorded the tree a node started from,
never the tree it left.

## Decision

1. **A node's finish names the run's tree as the node left it**, after whatever
   it landed there; a gate names the tree the person deciding saw.
2. **An invariant that runs a command on the tree** — a `bash` node, or a check
   that judges the tree — **verifies the tree it left.** Before the scheduler
   starts another node, drives a gate or finishes, it runs again the first
   such invariant, in declaration order, whose tree is no longer the run's,
   alone and with nothing else running.
3. **The run's tree is the newest one the log records**: any node's start, or
   the finish of a node that is not itself such an invariant. An invariant's
   own finish is left out, so one that rewrites files neither makes itself nor
   another stale, and two never send each other round.
4. **A re-verification that fails is an ordinary failure**: the node's
   `on_failure` re-routes under the same count, or a person decides, and what
   came after is not reached.

## Rationale

A check's verdict is about a tree. Running it again when the tree changed is
not a retry — the input it depends on changed — and it spends no session. It
removes the need for every workflow to repeat its checks after every node that
may edit code, which is the mistake the reference workflow made, and it covers
the cases no author can wire by hand: a corrective node reached by a re-route,
or a person editing the tree between two attempts.

## Rejected alternatives

**A rule in `yunta check` against a node that edits the tree between an
invariant and a gate.** It covers the static case and none of the others, and
it pushes every workflow to repeat its checks by hand.

**Counting an invariant's own finish as a change of the tree.** An invariant
that rewrites files — a formatter — would then make another stale, and two of
them could send each other round without end.
