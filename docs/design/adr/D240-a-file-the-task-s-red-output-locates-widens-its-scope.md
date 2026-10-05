---
number: D240
title: "A file the task's own red output locates widens its scope by itself"
status: accepted
revises: [D191, D73]
revised_by: []
---

# D240 — A file the task's own red output locates widens its scope by itself

## Context

A task's session that needs a file outside its scope meets the fence: the
write is refused, and the only way forward is a request. In `ask` mode that
request waits for a person; in `rules` mode it needs a proposed criterion,
and the rule refuses criteria that only restate the request. Yet the case
that stops a task most often carries its own proof: the build or the tests
fail, and what they print names the exact place — `crates/x/src/caller.rs:378:9`
— that the session tried to write and was refused. A person asked about it
has nothing to weigh that the engine could not check. D191 let only a
`rules` grant open another attempt on its own.

## Decision

1. **A located refusal is granted when the attempt ends.** When the fence
   refused a write in this attempt and a red criterion of the same attempt —
   one that ran, not one that could not — printed that path, relative to the
   checkout, followed by `:<line>`, the engine grants the path. The match is
   strict: the whole path, not the tail of another one nor its stem, and a
   line number right after it.
2. **A session may cite the evidence.** A request may name one of its task's
   own criteria as `evidence` instead of proposing one. The engine runs it on
   the session's work, through the run's memo, so the close does not run it
   again; when it is red and locates every path asked for, the request is
   granted. When it is not, the request is decided like any other.
3. **The grant is the evidence's.** `scope_expansion_granted` records
   `decided_by: evidence` with the criterion. It applies in `ask` and `rules`
   modes, never under `deny`; never to what no session may write; it counts
   toward `max_per_run`, whose exhaustion escalates as before.
4. **Another task's ground stays a person's.** A located path that another
   task of the batch may write is escalated instead of granted.
5. **The refused session goes on.** As with a rule's grant, the next attempt
   picks the same session back up, in the same checkout, told what was
   granted. This is a second case of D191's exception: something changed
   that the next session can use.

## Rationale

The engine decides what it can verify. A refused write that the task's own
failure points at is as checkable as a criterion: asking a person about it
stops the run for a decision with only one answer, and a planner cannot
foresee every such place.

## Rejected alternatives

**Matching a file's name or stem anywhere in the output.** `mod`, `lib` and
`main` appear in every build log; a loose match would grant whatever a
failure happened to mention.

**Granting located paths that were never refused.** A path the session did
not try to write is not one it needs: the evidence is the pair, the attempt
and the failure.
