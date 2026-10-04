---
number: D191
title: "A task gets one session per attempt; another opens on its own only when an engine grant changed what it may write"
status: revised
revises: [D146]
revised_by: [D192, D240]
---

# D191 — A task gets one session per attempt; another opens on its own only when an engine grant changed what it may write

## Context

The task cycle retried a task whose criteria stayed red with a fresh
session, up to two more times, before blocking it. D146 kept that budget as
the backstop for what a session's own checks did not catch.

In the runs of this repository that had tasks, four sessions opened that
way and three more on a person's `retry`, and none of the seven closed its
task. One task's criterion exited `101` identically three times. Another's
could not run at all (`127`) and spent six sessions. D187 records a third:
two retries on the same brief left the tree as it was. The one second
session that closed its task followed a scope grant — something had changed.

A session reads its task and judges its work through the engine's tools
while it works (D187). A session that ends red either believed it was done,
met something outside its reach — scope, environment, a wrong criterion —
or ran out of room. Another session on the same task, the same tree and the
same evidence has nothing the first did not, and each one costs millions of
tokens.

Retrying also hid a defect: what the engine granted during an attempt was
not carried to the next one, so under a fence the attempt a grant was for
was judged without it.

## Decision

1. **One session per attempt, and a red attempt blocks the task.** The
   cycle does not open another session on its own. The blocked task's
   escalation names what the attempt left red, and a person decides:
   `retry`, `continue-work` or `grant`.
2. **The one exception is an engine grant.** When `rules` mode grants a
   scope expansion during an attempt, the fence refused that write in the
   attempt that asked, so the next attempt is different by construction and
   opens on its own. It is the only one; what the grant bounds is bounded
   by `max_per_run`.
3. **What was granted holds from then on.** Every attempt is judged, and
   fenced, against the task's scope plus everything granted before it: on
   the log when the cycle began, and during the cycle's earlier attempts.
4. **The retry budget is gone.** No cap, no default, no knob.

## Rationale

A re-execution earns its cost when something it depends on changed. A
grant changes what a session may write; a person's decision may come with a
fix to the tree or the environment; a sibling's integration changes the
tree under a task. A repetition with nothing changed spends the most
expensive resource on the answer the last one gave.

## Rejected alternatives

**A configurable budget with a default of zero.** It keeps a mechanism the
evidence does not support, and a knob that exists invites turning it up
after the one run where it seemed to help.

**Retrying only on a failure the session marks retryable.** A session's own
word does not change the task, the tree or the evidence; it already ends
the cycle when it marks a failure non-retryable.
