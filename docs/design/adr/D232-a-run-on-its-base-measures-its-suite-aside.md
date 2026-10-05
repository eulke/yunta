---
number: D232
title: "A run born on its base commit measures its suite aside, and only what reads the measurement waits for it"
status: revised
revises: [D176]
revised_by: [D237]
---

# D232 — A run born on its base commit measures its suite aside, and only what reads the measurement waits for it

## Context

D176 has the lineage's root measure its suite on its first wake, before its
first node. Nothing else of the run could start meanwhile: on real runs the
measurement alone took more than ten minutes, spent while the planning nodes —
which read nothing the suite measures — sat waiting for it.

The wait was there for one reason: the measurement is of the tree the run opens
on, and any node may change that tree. A run with a checkout of its own, born on
exactly the commit it opened on, has that tree in another form too: any other
checkout of the same commit.

## Decision

1. **A run records at birth whether it opens on its base.** `run_created` says
   whether the run was born in a checkout of its own holding exactly its base
   commit. The scheduling policy reads it off the log, so every process deciding
   about the run decides alike.
2. **Such a run measures aside.** As soon as it owes the measurement, its suite
   runs in a checkout of the run's pool at the base commit, with no branch,
   while the run's steps go on.
3. **Only a reader waits.** The scheduler holds back a step only when it reads the
   measurement: a node that compares against it, holds tasks to it or hands it
   to a run it gives birth to — a group holding any of these — a gate that puts
   a plan in front of a person, and a question that may promote the run, whose
   successor is born holding the measurement. One rule says what reads it, and
   the same rule decides whether the run measures at all.
4. **A measurement cut short starts again.** An invocation that ends before the
   suite answers stops it and records nothing; the next wake measures again, in
   a checkout that keeps what the stopped suite built.
5. **Everything else measures in place.** A run working in a person's checkout,
   or one whose tree held more than its base, measures in its own tree before its
   first node, as D176 decided.

## Rationale

The measurement answers what passed before the invocation started; where it is
taken does not change the answer as long as the tree is the same, and a checkout
of the base commit is that tree. Waiting only where the answer is read keeps
every guarantee D176 gave a comparison while the planning of the run goes on.

## Rejected alternatives

**Decide at each wake whether the tree still holds the base.** A process that
answers a gate from outside would decide without that check, and two processes
could reach different decisions on one log.

**Measure on the run's own tree in parallel.** The nodes running meanwhile may
change it under the suite.
