---
number: D205
title: "A node's scope may be what the run changed"
status: accepted
revises: []
revised_by: []
---

# D205 — A node's scope may be what the run changed

## Context

A node that corrects the run's own work — the fix a failing lint re-routes to
— needs a scope: it should repair what the run did, not rewrite the project,
and its scope is what says so. Written as globs, that scope is a guess about
the repository: fragua's `fix-lint` declared `**/*.rs`, which means nothing
in a TypeScript repository. A pack cannot name the files of a repository it has
never seen, but it can name the run's own work.

## Decision

1. **`scope: run`** declares that a node may change what the run has changed
   since its base commit, as it stands when the node's attempt starts. The node
   gets a checkout of its own and is audited at close like any scoped node.
2. **The engine resolves it once per attempt, when the attempt starts** — the
   paths that differ between the run's base commit and the tree the attempt
   starts from — and records them on `node_started` as `run_scope`. The close
   audits against what was recorded, never against what the run changes later.
3. **A write elsewhere is a scope violation**, and a person may grant it as for
   any declared scope. That is the right signal when a lint was red before the
   run: fixing what the run did not touch is a decision, not a correction.
4. **Rules that compare scopes before any run exists treat `run` as reaching
   anything**, since what the run changed is known only when the node starts: a
   node scoped to the run overlaps any writing sibling of its `parallel` group.

## Rejected alternatives

**A project-level vocabulary of source sets (`sources.code`).** It would make
every pack and every project agree on names for parts of a repository, and a
pack would fail wherever a project named them differently.

**Scoping to the files a failing check's report names.** It would have to parse
every tool's report format, which is the coupling this removes.

## Consequences

A node scoped to a run that has changed nothing may change nothing: on an entry
node, `scope: run` is useless, and the guide says so. What a person edits during
a pause is part of what the run changed, so the next attempt may fix it.
