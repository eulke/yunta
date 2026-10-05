---
number: D243
title: "A person's decision to run a node again asks every failure again, and keeps what passed"
status: accepted
revises: [D230]
revised_by: []
---

# D243 — A person's decision to run a node again asks every failure again, and keeps what passed

## Context

D230 keeps a run's criteria answers across its invocations, for the tree and
the environment they were asked in. A real run then blocked twice on the same
thing: a guard came out red because of a fault outside the task's tree, the
person who saw it chose `retry`, and the retried attempt reused the red answer
recorded for the same tree — the task blocked again without running the guard.
The menu promised a fresh attempt. Nothing told the task cycle that a person
had decided, and the answer had no way to be asked again: the memo knows a
command and a tree, never what happened in the world around them.

## Decision

1. **A decision to run again is new information.** When a person chooses to
   run a node again after its failure — `retry`, `continue-work`, `grant` —
   the attempt that decision starts begins by letting go of every red answer
   the run holds, whether this invocation found it or took it in from the log.
2. **What passed stays passed.** Green answers stay: re-running them would
   cost the run its time and tell the person nothing about what failed.
3. **One place applies it.** The scheduler already knows an attempt starts on
   such a decision; the run acts on it there, so a decision made at the run's
   console and one recorded from outside while it was parked behave alike.
4. **The menu says it.** `retry` says that what failed is asked again and what
   passed stays passed.

## Rationale

A person retrying is the one signal that something outside the tree changed:
the network came back, a service was restarted, a file outside the checkout
was fixed. Keeping the red answer refuses the person's own decision. Asking
every red again — not only the node's — keeps the rule simple and true to what
the person said; the cost is one run of each red command, once, on a decision
a person made.

## Rejected alternatives

**Forgetting only the failed node's answers.** An answer does not know which
node asked it, and the same command judges several; tracking that would add
state to every answer for no case the simpler rule gets wrong.

**Letting the retried attempt run everything again.** It would re-run the
suite and every green criterion, minutes of work a person did not ask for.
