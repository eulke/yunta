---
number: D236
title: "A run works in a checkout of its project's pool, reserved by its branch for as long as it lives"
status: accepted
revises: [D158]
revised_by: []
---

# D236 — A run works in a checkout of its project's pool, reserved by its branch for as long as it lives

## Context

D234 moved the checkouts of a run's units, probes and measurement into the
project's pool, where a build stays warm from one run to the next. The run's own
checkout — where its nodes without a scope work, where units land, and where a
workflow's lint and tests run on everything the run did — was still made fresh for
each run under a path named after it, so its first build was cold every time. A
project worked around that by sharing one build directory between checkouts,
which D234 explains is wrong.

## Decision

1. **Born in the pool.** A run with a tree of its own takes a checkout of its
   project's pool, put on a branch of its own cut from its base: the free one
   nearest to that commit, or a new one. A child run does the same.
2. **Reserved by its branch.** A checkout on a run's branch is that run's for as
   long as the run lives, parked or not, even holding nothing but its base. No
   unit and no other run is handed it.
3. **Given back at its end.** Whatever its end — done, failed, closed — the run's
   checkout goes back to the pool on no branch, unless something in it is
   uncommitted. The branch stays: it holds what the run did. `cleanup: worktree`
   deletes it once its work is in that checkout's commit.
4. **A successor carries on in it.** A promotion's successor cuts its branch where
   its predecessor's tree stands, in the same checkout.
5. **A wake takes one again when it has to.** A woken run works in the checkout its
   log names while that checkout is still on the run's branch, or on a branch of a
   person's. When it is not — given back, taken by another, gone — the run takes a
   checkout of the pool onto its branch and `run_resumed` names it. A session that
   worked in the run's tree started where the run no longer is, so it starts
   afresh and the log says so.
6. **Parked too long, given back.** A run parked beyond `storage.retention_days`
   gives its checkout back when it is clean and no session waits to be picked up
   there; `gc` does it.
7. **`gc` gives back, never takes away.** A collected run's checkout of the pool
   goes back to it; only a checkout a run made before its project kept checkouts is
   removed.

## Rationale

The run's own checkout is the one every workflow builds in last — lint and tests
over everything the run did — so a cold one costs every run a full build at its
end. Reusing a checkout at its own path keeps that build right and warm, the way a
developer's checkout stays warm across branches.

## Rejected alternatives

**Finding the run's checkout only by its branch.** A person may switch it to a
branch of their own while the run is parked; the log names it instead (D235).

**Keeping a parked run's checkout forever.** Every abandoned run would pin a
checkout's worth of disk.
