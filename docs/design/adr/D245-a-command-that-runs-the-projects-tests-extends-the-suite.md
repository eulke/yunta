---
number: D245
title: "A command that runs the project's tests extends the suite the run measures"
status: accepted
revises: []
revised_by: []
---

# D245 — A command that runs the project's tests extends the suite the run measures

## Context

A run measures its suite once (`baseline.suite`), and every task of a loop is
held to it as a guard. The plan's own criteria and the spec's tests run the
project's tests too, and a planner writes them the way it would by hand: one
package at a time, `cargo test -p engine --test retry`. A test runner settles
what it builds for each invocation — Cargo unifies features per invocation — so
that command builds the project again beside the suite's build. Measured on this
repository, the suite's build takes 15 GB and 145 s cold. The same criterion
written as `cargo test --workspace --test retry` costs about one second and no
bytes once the suite has built. Per package, it costs 24 to 91 s and up to 12 GB
more. One checkout of a real run reached 39.5 GB this way.

## Decision

1. **A command that runs the project's tests extends the suite.** The suite's
   first command, without the variables it assigns, is what such a command
   starts with. Its words before the first option — `cargo test` in `cargo test
   --workspace`, `make test`, `pytest` — say what runs the project's tests.
2. **A plan's criterion and a spec's test are held to it when handed over.**
   Each shell segment that starts with those words and not with the whole
   command is refused as `outside-the-suite`, naming the command and how to
   write it. A command of another program is not held to it.
3. **The session writing a plan or a spec is told the suite.** Its contract
   names the command it extends.
4. **Only a run that measures a suite holds anything to it.**

## Rationale

The suite's build is the one the run already pays for, and every task needs it
warm. A criterion that extends the suite reuses that build; any other way of
running the same tests doubles the cost in time and in disk without telling
anyone. The rule reads only the command line, so it holds for any runner the
project uses and needs no knowledge of how a tool builds.

## Rejected alternatives

**Rewriting the planner's command into the suite's.** The engine would be
deciding what a test selects, and a rewritten command is not the one a person
approved.

**Deduplicating builds across checkouts.** Measured: two builds of the same
commit share 0.7 of 23 GB. A build is right only at the path that made it.

**Leaving it to the prompt.** A prompt asks; a rule refuses. The real runs
measured for this decision wrote per-package criteria with a prompt that only
asked for the project's own test runner.
