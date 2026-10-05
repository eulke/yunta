---
number: D248
title: "A run follows the build its units leave"
status: accepted
revises: [D235, D236]
revised_by: []
---

# D248 — A run follows the build its units leave

## Context

D236 gave a run a checkout of its project's pool, warm with whatever the last
run there built, and kept it for the run's whole life. Measured on this
repository, a run whose work is done by units builds almost nothing in that
checkout until the units' work has landed. Then its lint and tests build the
landed commit from cold: 48 s of lint and 145 s of tests. At that moment, the
checkout of the last unit holds a build of that same commit, and it is free.
Each run also took a warm checkout for its own tree, which then sat idle while
its units and its measurement built in colder ones.

## Decision

1. **The run moves into the build.** Between two steps, when units' work has
   landed since the run took its checkout and since a node last started
   building in it, the next node that builds in the run's tree runs in the
   free checkout of the pool that stands exactly at the run's commit and holds
   the most build. The run moves only while nothing of it runs or measures,
   when its tree is clean on its branch, when the other checkout holds every
   kind of build the tree holds (what git ignores, by top-level name), and
   when that checkout holds more of it.
2. **By its branch.** The tree lets go of the run's branch and the other
   checkout takes it. If git refuses the second step, the first one is undone
   and the run stays. The old tree goes back to the pool with its build
   counted.
3. **The log names it.** `run_moved` names the new checkout, and the run's
   ledger reads it as the run's checkout, as it reads `run_created` and a
   `run_resumed` that moved (D235). A crash between the switch and the event
   leaves the run's branch in the new checkout, and the wake finds it there
   (D236, point 5).
4. **The invocation carries over.** The run goes on in the same process, with
   what its invocation answered, spent and holds. A successor and a parent's
   view of a child take the run's tree from where it stopped, not from where
   it began.
5. **Born light.** A run whose workflow lands units' work, through a loop or an
   audited node, is born in the free checkout holding the least build. The
   warm ones stay for its measurement and its units. A wake that has to take a
   checkout again takes the warmest one at the run's commit. Every other run
   is born in the warmest one, as D236 says.

## Rationale

A build is right only at the path that built it, so the build cannot come to
the run. The run can go to the build, by its branch, which is what owns a
checkout for a run (D236). The move costs about seven git commands, once per
landing, against a whole build of the commit. Being born light is what makes
moving pay twice: the warm checkouts serve the work that builds first.

A session picked back up after a move resumes from the new checkout. The
agent CLIs this was measured with find a session from any checkout of the same
repository (Claude Code from 2.1.223) or from anywhere (Codex).

## Rejected alternatives

**Moving the build into the run's tree.** A build names the directory it ran
in, and build tools judge their output by its path and its time. A moved build
is rebuilt, or worse, trusted at the wrong path.

**Running every node of the run in a unit.** It would give each node a warm
checkout, but the run's tree is where units land and where what git ignores
passes from one node to the next. Moving the run keeps both.

**Moving only when the last node to finish landed units.** A gate between a
loop and the lint would keep a light-born run in a checkout holding nothing,
and its lint and tests would build from cold.
