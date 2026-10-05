---
number: D235
title: "A run's log names the checkout it works in"
status: accepted
revises: [D158]
revised_by: []
---

# D235 — A run's log names the checkout it works in

## Context

D158 had a woken run ask its tree two questions — is it there, and does it still
descend from the base — at "the path the manifest froze": the worktrees root,
joined with the run's id. Every reader composed that path again on its own:
`resume`, `status`, `gc`, a child's resume, a promotion. A run's checkout only
stays at a path derived from its id while every checkout is the run's alone. Once
checkouts belong to the project and a run takes one of them, the path is a fact
of that run's life, and only its log can say what it was.

## Decision

1. **Born naming it.** `run_created` carries the checkout of its own the run
   works in. A run working in a person's checkout has none to name.
2. **A wake that moves it says so.** When a run wakes in another checkout than
   the one it last worked in, `run_resumed` names the new one.
3. **One reading.** The run's ledger answers with the checkout its log last named,
   and every reader takes the run's tree from there. A log written before the
   field names none, and its run's tree is the one D158 named.
4. **D158's questions stand.** A woken run still asks the checkout its log names
   whether it is there and whether it descends from the base.

## Rationale

The log is where a run's facts live; a path composed by each reader is a guess
that holds only as long as a rule nobody records. Naming it once at birth and
again only when it changes costs nothing on a run that never moves.

## Rejected alternatives

**Finding the run's checkout by the branch it is on.** A person may switch
branches in a paused run's checkout, a run working in a person's checkout has no
branch of its own, and a reader outside the repository has nothing to ask.
