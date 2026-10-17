---
number: D256
title: "A project command held beside the suite holds every task, as the suite does"
status: accepted
revises: [D196]
revised_by: []
---

# D256 — A project command held beside the suite holds every task, as the suite does

## Context

D196 held every task of a loop to the suite its lineage measured green. A
project's lint is the same kind of promise — what was clean before the run
stays clean — but it was a node at the end of the workflow: a lint that
went red there sent a new session, with none of the context of the one
that broke it, to fix what it found, and in most real runs that session did
not manage and a person was asked. Planners added lint guards to tasks on
their own, each in their own words, sometimes for a lint that was red
before the run started.

## Decision

1. **`baseline.held` names project commands** measured beside the suite,
   on the same tree, each recorded as `baseline_held`. A run born of
   another holds the root's, as it holds the suite.
2. **Each that passed holds every task as a guard**, exactly as the suite
   does: in its pre-check, its session's checks, its stop, its close and
   its integration. It runs before the suite, so a red one answers first;
   one red at the base holds nothing; no task departs from it.
3. **The engine never names one.** What holds a task is what the project
   declares under `commands:`; `check` refuses a name it does not declare.
   A workflow's own lint node stays as a final invariant.

## Rejected alternatives

**A lint guard declared by the planner.** It is the plan's word for the
project's promise, may name a lint already red, and costs the plan tokens
for what the config already says.

**Only the final lint node.** It finds the regression after the session
that made it is gone.
