---
number: D196
title: "Every task is held to the suite its lineage measured green"
status: accepted
revises: [D65]
revised_by: []
---

# D196 — Every task is held to the suite its lineage measured green

## Context

A fragua run implemented three tasks. Each one's criteria named only the test
its change introduced, so each closed green. One of them changed how an
interactive menu drew its selected row, which broke five existing terminal
tests. Nothing noticed until the `baseline_compare` node after the loop. By
then no session that knew why the change was made was left: the check had no
re-route, the failure menu offered only running the same comparison again on
the same tree or stopping, and the regression had no owner.

The tasks schema already had the tool — a `guard` criterion that must stay
green — but whether a task carried one was the planner's call, and the guide
advised against repeating the whole suite per task because of its cost. The
engine already held the one fact that makes the guard meaningful: the lineage's
own measurement of its suite, taken before any node changed anything.

## Decision

1. **When the lineage measured its suite green, every task of a loop is judged
   against it as a `guard`.** The engine adds the measured command to the
   criteria the task is judged by, with a `proves` saying what it is there to
   show. The pre-check, the session's own `yunta_check_task`, the attempt's
   close and the integration all run it; `yunta_task` lists it.
2. **The document stays the planner's.** The guard is how the run judges the
   task, not what the task says: `task_registered`, a task's identity across
   re-plans, the plan a person reviews and the handover probe read the
   document as written.
3. **A task that declares the suite itself is judged by its own declaration**,
   and a measurement that was already red holds no task to it — no task can
   keep green what never was.
4. **An inherited measurement holds the child's tasks the same way**: a
   regression its parent made blocks the child's tasks before any session,
   which is what an inherited baseline already means for its comparisons.
5. **Suites run once per distinct tree, not once per batch.** The memo keeps
   one tree from running the suite twice; each attempt and each integration
   leaves a tree of its own, and there the suite runs.

## Rationale

A regression is cheapest to fix where it is made, by the session that made
it, inside the task's scope — or, when the fix belongs to a file outside it,
through the scope request a person answers, which is exactly the question
worth asking a person: may this change what the old tests assert? The cost is
the suite's duration per check instead of once per workflow, which is small
next to the sessions whose work it judges, and a shared build directory keeps
it from being a cold build per task.

## Rejected alternatives

**Asking the planner to declare the guard.** It depends on every planner
remembering, and a planner that forgets reproduces the incident. What the run
measured is the engine's fact; judging against it is the engine's job.

**A corrective node after `baseline_compare`.** A fixer that sees only which
tests fail knows neither why the change was made nor whether the old test or
the new behaviour is right, and with a scope over every source file it can
"fix" the regression by rewriting the assertions.

**Bisecting the task commits after the comparison fails.** It finds the owner,
but after its session closed, and costs a suite run per task anyway.
