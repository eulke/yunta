---
number: D206
title: "A project's denied paths are never written by a run"
status: accepted
revises: []
revised_by: []
---

# D206 — A project's denied paths are never written by a run

## Context

A pack guessed what no agent should touch with a scope ceiling:
`scope_expansion.within: ["src/**"]` on fragua's `implement`. That is a guess
about a repository the pack never saw — in a pnpm monorepo, whose code lives
under `packages/*/src`, not even a person could grant an expansion the work
needed — and it protected nothing it did not also block. What must never be
written by an agent is a fact about the project: its CI, the configuration of
its own checks, the files a team keeps for itself.

## Decision

1. **The project declares it, under `permissions.paths.deny`.** It is part of
   the permission ceiling: every layer's denies stand together, and no layer
   takes one away. Empty by default, so no workflow ever fails for lacking it.
2. **It is enforced where work is accepted into the run.**
   - A session's write fence refuses a denied path, whatever its scope, where
     its adapter fences exactly.
   - A node with a checkout of its own whose diff reaches a denied path fails
     without landing, with `denied_paths` as its failure.
   - A task whose diff reaches one is not integrated; its scope check records
     the denied paths apart from the rest of its violations.
3. **Nothing denied reaches the run's branch from its shared tree.** The close
   that commits the run's tree answers for the whole commit: what it would add
   that the project denies is refused and put back as the branch had it before
   the rest is committed, so the next node's start does not commit it as
   found. A node that finished fails with `denied_paths`; one that failed keeps
   its own cause and names what its close refused (`node_failed.refused`). A
   `parallel` group commits what its children wrote, so it answers for them. In
   a person's own checkout (`isolation: none`) nothing is committed or put back,
   and the node fails with what it wrote.
4. **Nothing widens it.** A failure over a denied path never offers `grant`; a
   task's scope-expansion request that reaches one is denied by rule before
   any mode is asked; a node's request for one is never put to a person.
5. **A pack never names it.** It is the project's policy, not a pack's guess:
   the portable scope for a pack is the one it can know (`scope: run`, D205).

## Rejected alternatives

**Keeping `within` as the pack's ceiling.** It is a guess about layout, it
blocks legitimate work in repositories laid out differently, and a pack has no
way to know what a project protects.

**Asking a person before a denied write.** A deny is a decision the project
already made; asking again turns a policy into a prompt a person learns to
click through.

## Consequences

A node without an exact fence may still write a denied path; its close refuses
it. What a person edits during a pause is committed as found even where the
project denies it: a person's edit is theirs, not the run's. Found work cannot
tell a person's edit from what an interrupted attempt left, so a denied path an
attempt left behind a crash is committed as found too. A project that denies a lockfile denies every
task that adds a dependency: the list is the team's call.
