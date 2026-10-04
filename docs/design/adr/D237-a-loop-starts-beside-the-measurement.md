---
number: D237
title: "A loop starts beside the measurement, and only a guard's verdict waits for it"
status: accepted
revises: [D232]
revised_by: []
---

# D237 — A loop starts beside the measurement, and only a guard's verdict waits for it

## Context

D232 let a run born on its base measure its suite aside while its first steps
ran, and held back only the steps that read the measurement. A loop was one of
them, because whether a task is held to the suite depends on the answer: a suite
red before the run changed anything holds nothing. The loop's tasks only need that
answer when a guard is judged, though — after a session's work, once the task's
own criteria pass. Waiting at the loop's start held every session back for the
whole measurement, minutes a run spent doing nothing.

## Decision

1. **A loop does not wait.** The scheduler holds back a step that compares against
   the measurement, hands it to a child, or shows a plan to a person; a loop, alone
   or in a group, starts beside the measurement.
2. **The suite holds the tasks provisionally.** While the measurement is under way,
   a task carries the suite as a guard, and its session is told the suite will
   judge it.
3. **A guard's verdict waits.** A check whose own criteria pass waits for the
   measurement before it judges the suite: the session's own check, the check after
   its work, the check after its work is replayed onto the run's tree. A suite that
   passed holds the task; one already red is dropped from the check. The check
   before the work leaves the suite out while it is being measured: the measurement
   is that answer, on the tree the task starts from.
4. **A measurement that stops without an answer** releases every waiting check,
   which runs the suite itself, as it would with no measurement at all. A
   cancellation that lands while a check waits cuts the attempt like any other.

## Rationale

The measurement answers one question — did the suite pass before the run changed
anything — and the only reader that needs it is a guard's verdict. Moving the wait
there keeps every guarantee D232 gave and returns the measurement's time to the
sessions.

## Rejected alternatives

**Starting the loop and judging guards without the measurement.** A suite red at
the base would then block every task for a failure none of them made.
