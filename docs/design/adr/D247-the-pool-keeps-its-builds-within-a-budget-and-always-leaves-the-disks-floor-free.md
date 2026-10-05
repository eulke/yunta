---
number: D247
title: "The pool keeps its builds within a budget, and always leaves the disk's floor free"
status: accepted
revises: [D234]
revised_by: []
---

# D247 — The pool keeps its builds within a budget, and always leaves the disk's floor free

## Context

D234 had `gc` keep as many free checkouts as were busy at once in the last
30 days. It counted checkouts, but a checkout's weight is its build:
measured on this repository, a checkout's source takes 9 MB, and its build
takes 15 GB for the suite alone and reached 39.5 GB under a real run's mix
of commands. Two real runs left a pool of 60 GB, with nothing measuring it
and nothing stopping it short of a full disk. Space is performance too: a
run that fills the disk fails in the middle of whatever it was doing.

## Decision

1. **A checkout's build is measured, off the run's way.** Giving a checkout
   back records where its build is (what git ignores) and counts its bytes in
   the background. The next process that surveys the pool finishes a count
   nobody finished, and `gc` does too.
2. **The builds stay within a budget.** `storage.pool.budget`, as bytes or as
   a share of the disk. By default it is the smaller of a fifth of the disk
   and three times the largest build measured: a task's, a measurement's and
   the run's own, each warm. More builds than that would stand idle.
3. **The disk always keeps a floor free.** `storage.pool.keep_free`. By
   default it is the larger of 5% of the disk and 10 GB.
4. **Room is made by emptying free builds.** Before a checkout is taken, the
   builds of free checkouts beyond the budget, or under the floor, are emptied
   with `git clean -X`, the least recently used first. The checkout about to
   be taken is never emptied, nor a held one, nor one kept for a unit or on
   a branch. The source stays.
5. **Otherwise the run stops.** A checkout whose build would still leave the
   disk under its floor is refused. The run pauses saying how much is free,
   how much the build takes and how to make room, and `yunta run` creates no
   run. The floor is checked before every batch as well.
6. **`gc` tidies and reports.** It counts what nobody counted, empties the
   builds nobody used within retention, applies the budget, removes free
   checkouts that hold no build, and takes away the clean checkouts runs
   nobody drives kept before the pool. It says in one line what the pool
   keeps, and `doctor` checks the same.

## Rationale

The disk is shared with everything else the person does, and a build is the
only part of a checkout worth limiting. Emptying a build costs a rebuild,
while filling the disk costs the run and maybe more. The floor is what makes
the guarantee hold even when builds were never measured.

## Rejected alternatives

**Counting checkouts, as D234 did.** One checkout can weigh more than ten.

**Measuring each build as it is given back, in line.** The walk takes seconds
on a large build, and a run's landing would wait for it.

**Emptying a parked run's own checkout.** A run's checkout is not held
between its steps, so emptying a run's build could pull it out from under a
step about to run.
