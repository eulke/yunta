---
number: D264
title: "A run waiting on a limit's reset reads waiting and exits 5, not needs you and 3"
status: accepted
revises: [D214]
revised_by: []
---

# D264 — A run waiting on a limit's reset reads waiting and exits 5, not needs you and 3

## Context

D262 makes a limit's reset a scheduled resume: the run pauses until the reset and
resumes on its own then. D214 gave every stop a word and an exit code, and a run
stopped by a limit was `needs you` and exited 3, the code a script reads as "a
person has to act". Nothing is asked of a person here: the wait ends by itself,
and a surface that said `needs you` sent the reader to a decision that did not
exist.

## Decision

1. **A run paused on a limit's reset is `waiting`.** It is a run word of its own
   on every surface and in the JSON. Its chronicle line reads `waiting —` with the
   sentence the pause always had, and the live region draws no row asking for a
   person while it waits: its footer says `waiting for a reset`.

2. **It exits 5.** `run` and `resume` exit 5 for `waiting`. Exit 3 keeps its one
   meaning: a person has to act. A `yunta test` case may expect `waiting`.

3. **The status block names no command for a reset.** It says the run resumes on
   its own, since a wait is ended by the reset and nothing typed.

4. **`waiting` joins version 7 of the JSON documents, and the version does not move.**
   No released binary wrote version 7, so `stats --json`, `status --json` and
   `run --json` keep `schema_version: 7` and carry the word in that version.

This revises D214: its run words gain `waiting`, its exit codes gain 5, and 3 no
longer covers a reset.

## Rationale

A script that stops on code 3 pages a person for a wait that ends by itself, and a
reader who sees `needs you` looks for a decision that is not there. The word and the
code say what the sentence already says: the run resumes on its own at the reset.
Keeping 3 for the stops that need a person keeps that code's meaning whole. The
stamp stays at 7 because version 7 has not shipped: a version 8 would mark a change
that no released document has yet carried.

## Rejected alternatives

**Keep `needs you` and exit 3.** A script or a reader acts on a stop that needs
nothing.

**Exit 0 for the wait.** Code 0 says the run finished, and a script would read a
parked run as done.

**Exit 1 for the wait.** Code 1 says the run failed or broke, which is untrue of a
run that will resume.
