---
number: D195
title: "A plan a person reviews explains itself, in the same document the engine runs"
status: accepted
revises: []
revised_by: []
---

# D195 — A plan a person reviews explains itself, in the same document the engine runs

## Context

A gate can show the plan it asks a person to approve (D194). The tasks document
was written for the engine: ids, titles, scopes and commands. The person reading
it at the gate learned what would be run, but not what would change and why, how
the work was approached, which types or interfaces it would create or modify, what
each command proved, or what it left out.

The tasks spec kept the document to one top-level key, `tasks:`, with no header
metadata: the brief, the mode and the run already live in the manifest and the
log, and a smaller schema has less to validate.

## Decision

1. **The tasks document carries what a person reviewing the plan reads.** Beside
   `tasks:` it may carry a `summary`, a Markdown `description` (code blocks for
   examples or how the parts interact, `mermaid` blocks for diagrams), a
   Markdown `design` with the shapes the plan creates or changes, `risks` and
   `out_of_scope`. Each task may carry a Markdown `description` and each criterion
   what it `proves`. The brief, the mode and the run stay out.
2. **One document for both readers.** What a person approves, by its hash, is what
   the engine executes. No second document can say one thing while the tasks do
   another.
3. **Required when a person will read it.** When a gate among the nodes the run's
   mode includes shows the plan, the summary, the plan's and every task's
   description, and every criterion's `proves` are required. The engine refuses
   the document at the handover and at the node's close, naming each missing piece.
   `design`, `risks` and `out_of_scope` stay optional: a plan that only touches
   documentation creates no shape.
4. **A view for reading it whole.** When it accepts a tasks document, the engine
   writes `tasks.md` beside `tasks.yaml`, derived from the same bytes. It contains
   the planner's text as written, what the engine knows of the plan (its order,
   what it touches, the guards it holds, a diagram of its dependencies), and a
   table per task of what each criterion proves. The console shows a compact
   version and points to the view.

## Rationale

The person at the gate decides on intent, approach and consequences, and only the
planner can state those. The engine can add what it computes from the document, so
the planner writes only what is judgment. Keeping both in the document the engine
runs means the explanation is reviewed and approved together with the tasks it
explains.

## Rejected alternatives

**A separate `plan.md` written by the planner.** It needs no change to the engine,
but it is a second source: nothing checks that the prose describes the tasks the
engine will run.

**A plan document the engine derives the tasks from.** It would change the loop's
and the handover's contract for what one document with optional fields already
gives.

**Always requiring the explanation.** A workflow that runs a document nobody
reviews, like a tasks document handed in as an input, would pay for text no one
reads.
