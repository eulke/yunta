---
number: D210
title: "A run whose engine is gone reads as stalled"
status: accepted
revises: []
revised_by: []
---

# D210 — A run whose engine is gone reads as stalled

## Context

A run's phase is derived from its log, and a log only says what happened. When
the process driving a run dies without writing a pause — `kill -9`, a crash, a
machine that lost power — the log stops at a node that started, and every
surface that reads it from outside said `running`: `status`, `list --runs` under
"in flight", and the control plane's `workflow_status`. Nothing told the person
that no process was driving it, or that `yunta resume` would pick it up.

`yunta cancel` already knew the difference: it reads the run's registry
(`scratch/engine.json`), which names the engine's pid and when it started, and
asks the host whether that process is still the one that wrote it.

## Decision

1. **A run whose phase is `created` or `running` and whose registry names a
   dead engine reads as `stalled`** on every surface that reads it from outside:
   `status`, `status --json`, `list --runs` (in a group of its own, after what
   needs a person) and `workflow_status`.
2. **Only proof counts.** A dead engine is a registry that names a process that
   exited, or a pid the host gave to a process that started after the registry
   says the engine did. No registry is not proof — a run handed from one process
   to another has none for an instant — and a registry that does not read is
   not proof either.
3. **It is a reading, not an event.** Nothing is written to the log; the reader
   asks the host each time.
4. **`status` names the way back**: `yunta resume` continues the run from its
   log, and `yunta cancel` records where it stopped.
5. **The invocation that drives a run never calls it stalled**: it is the engine.

## Rationale

A person reads `running` as "leave it alone". For a run nothing is driving,
that is the one wrong instruction. The registry is already the answer `cancel`
trusts before it signals a process; reading it for display reuses the same
proof instead of inventing a second one.

## Rejected alternatives

**Writing a `run_stalled` event.** Nobody is there to write it: the process that
could is the one that died, and a reader that wrote it would be writing a fact
about a moment it did not witness.

**Calling a run with no registry stalled.** It would be wrong for every run in
the instant between `run_created` and the detached engine writing its registry,
and for every hand-off a decision triggers.
