---
number: D233
title: "A plan the run writes a spec for is proven by its spec: handed over, it is only asked whether its programs can be found"
status: accepted
revises: [D227]
revised_by: []
---

# D233 — A plan the run writes a spec for is proven by its spec: handed over, it is only asked whether its programs can be found

## Context

A plan was proven when it was handed over by running every criterion of every
task where the engine runs criteria; a criterion that passed before any work was
refused (D227 point 3). In a run that writes a spec, the tests those criteria run
are the spec's to write — nobody has written them when the plan is handed over. A
test filter that matches no test passes, so the plan was refused for the very
gap the spec is there to fill, and its planner learned to work around the rule
instead.

The spec was then proven with every file of the document written into one
checkout, while each task's work starts from the run's tree with only its own
files in it. The trees differed, so the loop's first check of every task ran its
tests again — for a project whose tests compile first, the most expensive check
of the run, paid twice.

## Decision

1. **A specified plan is not run when it is handed over.** When the run's
   workflow writes a spec, a plan is held to the rules D227 lists and to one more
   thing that can be told without running it: every program its criteria start
   is one the engine finds, looked up under the run's shell and `PATH`. A program
   named by a path is left to the spec.
2. **The spec proves each task where its work starts.** Each task's tests run in
   a checkout of the run's tree with that task's files in it and no other's —
   the tree the loop's first check of that task reads, which then takes those
   answers from the memo.
3. **A plan with no spec is proven as before.** Its criteria run when it is
   handed over, and one that passes before the work is refused with what it
   printed.

## Rationale

The spec is where a run proves its tests fail before the work; asking the plan the
same question earlier asks it of tests nobody wrote. What the plan can be held to
then is what does not depend on them: that its criteria start programs that exist.
Proving each task on its own tree is also what makes the proof reusable: the
answer is for the tree the task meets.

## Rejected alternatives

**Run the plan's criteria and ignore their verdict.** It costs the run of every
criterion — a build, often — to learn only whether its program exists.

**Prove the spec with every file in and the loop with every file in.** A task's
work would then answer for tests other tasks are held to.
