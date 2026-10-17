---
number: D263
title: "A retry claims only the change its precondition names"
status: accepted
revises: [D243]
revised_by: []
---

# D263 — A retry claims only the change its precondition names

## Context

D243 makes a person's decision to run a node again ask every failure again, and
it reads that decision as the one signal that something outside the tree changed.
The menu then offered `retry` after any failure. The reading holds for a cause
outside the tree: a credential was fixed, a service came back. It does not hold
for a cause the tree decides. A red check, an artifact or an unmet criterion on a
tree the attempt did not change gets the same answer from the same input, and a
person's retry there tells the log that the world changed when it did not. A plan
whose criterion no work can pass has the same problem: its cause lies in the plan,
and a retry leaves the plan as it was.

## Decision

1. **A retry for an outside cause is a person's retry in D243's sense.** A cause
   outside the run's tree — a credential, a model, the service, a limit that names
   no reset time, a command that cannot run, the environment — is retried under a
   precondition that names the outside input. A limit that names its reset is a wait
   instead (D262). The person's retry claims that this input changed, and D243's
   rules hold for it.

2. **A retry for a tree cause claims only what the log shows.** A red check, an
   artifact or an unmet criterion is retried under the precondition that the tree
   changed, and only when the newest record of the run's tree names a tree other than
   the one the failed attempt left: a node's start or close, or a wake that found the
   tree (`run_resumed.tree`). A person's change to a parked run's tree reaches the log
   through that wake, so the resume that follows the change is what offers the retry.
   Such a retry is not a person's claim about the world outside the run. It rests on
   the tree's own move, which the log records.

3. **A tree cause on an unchanged tree offers no retry.** The same question on the same
   tree has the same answer, so no retry is offered until the tree moves. A guard red
   before any work is not a tree cause while the tree has not moved: what its command
   reads may lie outside the tree, so point 1 applies and its retry waits for that
   input. Once the log has seen the tree move, its retry waits for the tree, as point 2
   says.

4. **A plan no work can pass offers only abort.** Its cause is the plan's criterion,
   which the planner rewrites through the route D260 gives it. A retry leaves that
   criterion where it was.

5. **This revises D243.** D243 offered a person's retry after any failure and read
   every such retry as a change outside the tree. Under this decision the retry is
   offered for the causes above, and each retry claims the change its precondition
   names: an outside input for an outside cause, the tree for a tree cause.

## Rationale

A person's retry is useful because it says something the log cannot show: that
something outside the run changed. When the cause is in the tree, a retry says
nothing the tree does not already show, and when nothing changed at all it asks
the same question twice. Offering a retry only where its input can change keeps
every retry honest in the log, and the menu names that input, so the person knows
what to fix before deciding.

## Rejected alternatives

**Keep D243's reading for every retry.** A person's retry for a tree cause would then
claim an outside change the log cannot show, and a later reader would look outside
the run for a cause that was in the tree.

**Offer a retry for every failure and let the attempt show the same answer.** The
attempt would spend a session or a command run to learn what the log already holds.

**Drop the retry for tree causes.** A red check that a person has fixed in the tree
is a real way forward, and the tree's move is what makes it one.
