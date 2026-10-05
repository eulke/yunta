---
number: D187
title: "A task session reads its task and checks its work through the engine's tools; each check is on the log when it runs"
status: accepted
revises: []
revised_by: []
---

# D187 — A task session reads its task and checks its work through the engine's tools; each check is on the log when it runs

## Context

The contract (§5.2 step 3) gives a task session a minimal brief: the tasks
document and the task's id, from which "the agent reads its task at the
moment". The code sent the node's instruction, the task's id, title and
notes, and no way to reach the document. A document produced by a `plan`
node lives with the run, not in the checkout, so a session told to "read
your task from the tasks document" had nothing to read.

Run `01M3DMP8W9R4PZG9TWAPX4CRZ7` showed the cost. The executor never learned
its task's criteria or scope. It did not write the test file its criterion
named, so the criterion stayed red, and it edited files that belonged to
later tasks. Its own summary said the document was not in the checkout. Two
retries got the same brief and left the tree as it was, spending 1.2M input
tokens, and the task was blocked.

A retry could not have been told why either. The cycle kept every pre-check,
post-check and scope audit in memory, and integration wrote them all at once
after the last attempt. Nothing on the log described attempt 1 when attempt 2
opened.

## Decision

1. **A task session reads its task through `yunta_task`.** The tool answers
   with the task as the cycle judges it: id, title, notes, the criteria, and
   the scope the diff is held to (declared plus granted). It also returns every
   check the log holds for the task's current cycle. The brief carries only
   which task is the session's; the sentence that points at the tools is
   produced by the mount (D148).
2. **A task session checks its work through `yunta_check_task`.** It runs the
   judgement an attempt's close makes — the criteria on the tree as it stands,
   then the scope audit — through the same function, the same criterion cache
   and a private index of its own. A clean answer is what the close would give
   that tree, and the close reuses the cached results while the tree is
   unchanged.
3. **Each check is on the log when it runs.** The cycle records the pre-check
   before the task's first session, and each attempt's post-check and scope
   audit before the next session opens. Integration records only what it
   decides: fence breaches, scope expansion requests and the re-verification
   on the integrated tree.
4. **A loop requires the run tools.** A loop whose runner cannot hold them is
   refused before any session opens, as a blackboard group or an interpreted
   artifact already is (D156). A listener that fails to bind fails the task
   session instead of degrading.

## Rationale

The tasks document and the log remain the only record of what a task is and
where it stands. Rendering the task into the prompt would give the session a
copy that a grant or an earlier attempt's result could make stale. The tools
read the source at the moment they are called, which is what the contract
asked for, with a tool in place of a path.

A check that uses the close's own function cannot disagree with the close.
Recording checks as they run costs nothing in determinism: integration keeps
its serial, declaration order, and a task's own checks were never interleaved
with another task's in any way replay depends on.

## Rejected alternatives

**Render the task's scope and criteria into the brief.** It works with any
adapter, but it is a second copy of the task. It goes stale within a cycle and
cannot carry what a session asks mid-flight: the current verdict.

**Name the document's path in the brief.** It depends on the session reading
a file outside its checkout, and it still leaves no way to see an earlier
attempt's result or to judge the work before the session ends.

**Run tools as an offer for loops too, with an inline fallback.** A task
session without its tools works blind, which is the failure this decision
removes. Both real adapters declare the capability.
