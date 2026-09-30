---
number: D208
title: "A run measures its suite only when it reads the measurement"
status: accepted
revises: [D176]
revised_by: []
---

# D208 — A run measures its suite only when it reads the measurement

## Context

A lineage measured `baseline.suite` before its first node whenever the config
named a suite (D176). The suite is a project fact — `yunta init` detects it —
but whether a run needs it is the workflow's: a workflow with no comparison and
no loop paid for a full run of the suite on every run, and `check` could only
warn about it. Declaring the suite once for the project meant choosing between
paying on every workflow and editing the config for the one that compares.

## Decision

1. **A run measures only when something of its own reads the measurement**: a
   `baseline_compare` node, a `loop` whose tasks are held to the suite (D196), or
   a `kind: workflow` node, whose child is resolved only at its birth and may
   do either. The config naming a suite is still required; it is no longer
   enough.
2. **The rest of D176 holds.** The root measures once, on its first wake,
   before any node; a run born of another is born holding the root's
   measurement.
3. **`check` warns only where the measurement is still wasted**: a workflow
   that composes others, none of which compares, and that has no loop of its
   own.

## Consequences

A project declares its suite once, and only the runs that read it pay for it.
A mode that leaves the comparison out still measures when the workflow declares
one, so a promoted successor is born holding the root's measurement.
