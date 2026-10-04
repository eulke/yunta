---
number: D230
title: "A run's criteria keep their answers across its invocations, for the tree and the environment they were asked in"
status: accepted
revises: [D59]
revised_by: []
---

# D230 — A run's criteria keep their answers across its invocations, for the tree and the environment they were asked in

## Context

D59 kept the criteria memo inside one invocation: a resumed run started with a
cold cache and ran again every criterion it had already answered. A run that
pauses on a gate and is woken by its answer pays that on the first check of
every task — the project's whole suite among them, held to each task as a
guard. On real runs the first pre-check after a wake re-ran the suite for more
than ten minutes on a tree it had already answered for.

The log could not carry those answers forward: a check recorded what each
command answered, not the tree it answered for, and nothing said what the
commands ran with in the invocation that asked.

## Decision

1. **Every recorded answer names its tree.** A criterion's result in a check,
   in a session's check and in a document's hand-over names the git tree the
   checkout held, and, for a command that runs `git`, the commit it stood on. A
   measurement of the suite names the tree it measured.
2. **The log says what each invocation ran with.** The run's birth and every
   wake record the shell and `PATH` its commands run under, and the environment
   in force at any seq is read off the log.
3. **A wake takes what still speaks for it.** It seeds its memo with every
   recorded answer that names its tree (and commit, for a `git` command), that
   answered at all — a 127 or a 126 never — and that an invocation running its
   commands with the same environment wrote. A red answer keeps the object of
   what it printed and its last lines, so a check that reuses it still says why
   it fails.
4. **Still inside the run.** Nothing crosses to another run: another machine or
   another day is outside what the key can speak for.
5. **Reuse is visible.** A seeded answer reads `reused: true`, as any answer the
   memo gives.

## Rationale

The key already is the state: the same command, the same tree content, the same
commit for a command that reads history, the same resolved config. The one input
an invocation could change without touching the tree is the environment its
commands are looked up in, and that is now on the log. A wake that runs its
commands elsewhere runs them again.

## Residual risk

A program replaced in place — the same `PATH`, another binary behind it — between
two invocations of one run answers from the earlier binary's result. It is the
risk a run already takes within one invocation, extended to a pause.

## Rejected alternatives

**Carry only the suite's measurement.** It covers the most expensive answer and
leaves every task's own criteria and every hand-over probe to run again.

**Hash the programs on the `PATH`.** Correct against in-place replacement, but
reading every binary a command could run costs more than most criteria do.
