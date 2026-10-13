---
number: D259
title: "Within is retired, a refused request is refused whole, the cap counts only the engine's grants, and a person grants only instruction files"
status: accepted
revises: [D73, D189, D206, D238, D240]
revised_by: []
---

# D259 — Within is retired, a refused request is refused whole, the cap counts only the engine's grants, and a person grants only instruction files

## Context

Four rules about a scope request were settled differently in different places.

A loop's `scope_expansion.within` bounded what `rules` could grant, and a pack
could declare one, although no pack knows the layout of the project it runs in
(D206 rejected that for packs). A request that named a denied path still had its
other paths granted by evidence. Evidence granted `AGENTS.md` and `CLAUDE.md` when
nothing denied them, because no protection named them. And `max_per_run` counted
every grant, a person's answer included, so a person's answers could exhaust the
cap that bounds the engine's own widening of a run.

## Decision

1. **`within` is retired.** A workflow that still declares `scope_expansion.within`
   fails `check`, which names `permissions.paths.deny`. The scope of a node or a
   task says what it may write, and the project's denies say what no run may
   write; a loop has no ceiling of its own.
2. **A request is decided whole.** A request that reaches a protected path is
   refused as a whole, and the refusal names each path it reached and why, even
   when a red criterion locates the rest. The one exception is a request that
   reaches only `AGENTS.md` or `CLAUDE.md`: it waits for a person in `ask` and
   `rules`, and is refused in `deny`.
3. **`max_per_run` counts the engine's grants.** A grant the engine decides, by
   rule or by evidence, counts toward the cap; a person's grant does not. A grant
   the cap stops is escalated as before, and an exhausted cap never turns a refusal
   into an escalation.
4. **A person grants instruction files and nothing else.** A person may grant
   `AGENTS.md` and `CLAUDE.md`, at any depth, for the scope the request names. The
   engine grants no protected path, and no person grants the rest of the protected
   set: the project's denies, the configuration trees of each CLI, the approved
   tests and the git register.

## Rationale

A request is one object, the same whoever decides it (contract §6.2). A partial
grant would open the paths the engine can verify while the protected path the same
request names stays refused at the close, so the session would retry work the
engine already knew it could not finish.

`within` guessed at a repository's layout, the guess D206 rejected for packs, and
it blocked work in repositories laid out differently. A task's scope already bounds
what it writes, and the project's denies already bound what no run writes, so the
loop's ceiling added a guess and no bound.

The cap exists to catch a plan cut too finely: many grants in a row mean the engine
keeps widening a run by itself. A person's answer is not that widening, so it does
not spend the cap.

D206 says a project's deny is a decision the project already made, and asking a
person about it turns a policy into a prompt. An instruction file is different: a
person reads its content and judges the change, so the answer is a decision of its
own. The other protected paths are fixed before a run starts, and a grant during a
run would undo them one request at a time.

## Rejected alternatives

**Keeping `within` as a deprecated ceiling.** A ceiling a workflow may still declare
keeps the layout guess in place, and a warning would not remove it.

**Granting the located paths and leaving the protected one to a person.** The person
would decide a request that differs from the one the session made, and the granted
paths would be written before the decision they depend on.

**Counting a person's grants toward `max_per_run`.** The answers a person gives would
exhaust the cap that bounds the engine's own widening, and a run would pause for a
decision a person had already made.

**Letting a person grant any protected path.** The project's denies and the approved
tests would become answers that the next request could overturn.

## Consequences

- A workflow that declares `within` fails `check` until the key is removed. The field
  is read only so that the check can say where the ceiling moved; it is not written
  and it is not in the schema.
- A request that reaches a protected path is refused whole. Before this decision, the
  paths of it that were not denied, or that a red criterion located, could be granted.
- A run that resumes from a log with person grants counts fewer grants, so its cap
  opens earlier than it did under the old count. This is a change of behaviour, and
  this record names it.
- A log written before this decision keeps its text: a denial that carries
  `denial_reason` reads as a legacy reason, and a grant without a `mode` reads as
  having none.
