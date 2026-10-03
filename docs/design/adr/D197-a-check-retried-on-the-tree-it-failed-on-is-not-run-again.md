---
number: D197
title: "A check retried on the tree it failed on is not run again"
status: accepted
revises: []
revised_by: []
---

# D197 — A check retried on the tree it failed on is not run again

## Context

A node that fails with no re-route of its own, in a run that pauses on
failures, is put to a person: `retry` or `abort`. For a `baseline_compare`
that found a regression, `retry` ran the same suite on the same tree. Its
answer was known before it started — a check that runs a command on the tree
is as deterministic as a criterion — and the person learned nothing from the
minutes it took, except that the menu had offered a way forward that was not
one.

`retry` still has to stay on the menu. The person may fix the cause in the
run's tree while the menu is open, or while the run is parked, and the menu a
parked run is answered from is rebuilt from the log alone: it cannot run git to
see whether the tree moved.

## Decision

1. **A `retry` of a `check` that judges the tree is refused when the tree did
   not change.** When the attempt a person asked for starts from the same
   recorded tree as the attempt that failed, a `baseline_compare` or a
   `coverage_gate` fails at once without running its command, with the failure
   `unchanged: {since, failure}` — the attempt that ran and what it failed with.
2. **The menu is unchanged.** It still offers `retry`, so a person who then
   changes the tree hands the check back; a refusal of a refusal names the
   attempt that last ran.
3. **The rule is the check's.** `findings_gate` reads the log, not the tree, and
   a `bash` node also depends on its environment — a push that failed on the
   network can succeed on the same tree — so neither is refused.
4. **The tree is the log's.** The failed attempt's start tree is remembered when
   the next attempt starts, so the refusal is read from the log like every
   other decision.

## Rejected alternatives

**Dropping `retry` from the menu while the tree is unchanged.** The menu of a
parked run is rebuilt without running anything, so it could not tell, and the
live menu and the rebuilt one would disagree.

**Refusing every node kind on an unchanged tree.** A `bash` node's verdict is
not a function of the tree alone, and a prompt node's session is not
deterministic at all; refusing them would block the fixes a person makes
outside the tree.
