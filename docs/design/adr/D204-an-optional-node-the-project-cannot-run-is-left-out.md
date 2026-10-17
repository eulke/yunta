---
number: D204
title: "An optional node the project cannot run is left out"
status: accepted
revises: []
revised_by: []
---

# D204 — An optional node the project cannot run is left out

## Context

With project commands (D203), a pack names "the lint" and the project says
what runs for it. A project with no linter has nothing to say, and the
workflow is refused before the run: every key a node needs and the config
leaves unset is certain to fail the node, so `check` stops it. For a lint
step that is the wrong answer. A repository without a linter can still build
a feature, run its tests and open a pull request; the lint is a step the pack
would take where it can, not one the work cannot do without.

## Decision

1. **A top-level node may declare `optional: true`.** Where the project lacks
   a key the node needs (`ConfigKey`: a command, a forge, a runner…), the run
   leaves it out the way a mode leaves a node out: it is never scheduled, and
   its dependents wait on what it waited on.
2. **A node only it leads to goes with it** — one that waits on nothing,
   that nothing waits on, and that only left-out nodes re-route to. That is
   the fix a failing lint re-routes to.
3. **The run decides once, at birth, and records it** in
   `run_created.left_out`, with why: the keys the project lacks, or the node
   it was only reached through. Every wake, the scheduler, `status` and the
   receipt read it off the log; none re-reads a config. A binary that changes
   what a key means cannot change the graph of a run already recorded.
4. **Leaving a node out must leave the run whole.** `check` refuses
   `optional` on a child of a `parallel` group (the scheduler and modes read
   the top level), and on a node a node the run keeps re-routes to or reads
   from.
5. **An `invariant` may be optional.** A mode may never drop an invariant — it
   narrows deliberation, never verification. A project may still have nothing
   to run for one; then there is nothing to verify, and the run says so.
6. **A required node with an unset key is still refused before the run.**

## Rejected alternatives

**Skipping at run time, when the node is reached.** The graph would depend on
when a node was reached rather than on what the run was born with, and a
resume would have to re-read the config to know it.

**Excluding by mode.** A mode is the workflow author's choice of how much
deliberation a run gets; what a project has installed is not a mode.

## Consequences

`status` shows a left-out node with what the project lacks, and counts it
apart from the run's work. A workflow that marks a verification optional runs
without it in a project that declares nothing for it; the receipt and
`status` say so rather than implying it passed.
