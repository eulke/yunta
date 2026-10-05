---
number: D246
title: "A unit works on no branch: a ref keeps its work, and the pool's record keeps its checkout"
status: accepted
revises: [D234, D160]
revised_by: []
---

# D246 — A unit works on no branch: a ref keeps its work, and the pool's record keeps its checkout

## Context

D234 made a branch say two things about a unit: which checkout was the unit's,
and where its unconfirmed work lived. Every attempt of every task and audited
node cut a branch under `yunta/unit/`, and a repository a person works in
showed them all: a real run left dozens beside the person's own branches, and
every landing paid three git processes to detach the checkout and delete the
branch. A branch is the wrong place for either fact. Nobody but the engine
reads it, and the checkout's owner is already a fact the pool keeps beside
each checkout.

## Decision

1. **A unit works on no branch.** Its checkout is put on its base commit,
   detached. What it commits is kept by its ref,
   `refs/yunta/unit/<run>/<unit>/<attempt>`, outside `refs/heads`: no list of
   branches shows it, and git keeps what it points at.
2. **The pool's record says whose a checkout is.** `slot-N.held` names the
   unit the checkout is kept for. That unit's attempts take it back first. A
   checkout kept for another unit is anybody's only once that unit's work is in
   where the next user starts. A run still owns its checkout by its branch.
3. **Landing lets go of both.** When a task's work lands, its ref is deleted and
   its checkout is kept for nobody. A node keeps both until its run ends, since
   a person may send it back to its session.
4. **A run's end frees its units' checkouts.** A clean checkout kept for one of
   its units is kept for nobody, and the refs whose work the run's tree holds
   are deleted. A blocked task's ref stays, holding what it left, until `gc`
   collects the run.
5. **A session picked back up finds its checkout by the record.** Nothing is
   looked up by branch.
6. **Legacy unit branches are taken in.** A unit branch under
   `yunta/unit/` becomes its unit's ref, at the same commit, and a clean
   checkout of the pool on it is kept for that unit. A run's end does this for
   its own units, and `gc` does it for every run.

## Rationale

The ref keeps the work, and the record keeps the ownership. The record was
already where the pool decides who takes a checkout, so whose a checkout is
belongs there, read with the rest of the survey. The person's list of branches
goes back to holding their own branches and the runs'.

## Rejected alternatives

**Keeping the branches but deleting them sooner.** A blocked task's work still
needs a name that outlives the checkout, and any branch shows up in the
person's list.

**No ref, only the checkout.** A checkout taken by another unit is reset, and
the commits it held would be reachable from nothing.
