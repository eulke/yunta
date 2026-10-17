---
number: D186
title: "A `files:` entry may be declared optional; its absence is recorded, never silent"
status: accepted
revises: [D19]
revised_by: []
---

# D186 — A `files:` entry may be declared optional; its absence is recorded, never silent

## Context

D19 made a failed context source a failure of its node. For most sources that
is the only honest answer: a command that exits non-zero, an artifact the run
never produced or an MCP server that does not answer leaves the session
without something the workflow expected.

A `files:` entry is different when the workflow comes from a pack. The pack's
author names a path in a repository they have never seen. The reference pack
`yunta/fragua` reads `docs/architecture.md` in `plan` as a curated shortcut;
the planner works in a read-only session inside the repository and can explore
the code without it. In a repository without that file the node failed before
any session opened, after `grill` and `brief` had already spent their tokens,
and nothing the installer read had mentioned the file.

The engine cannot tell a shortcut from an input. Only the author knows whether
the node still makes sense without the file.

## Decision

**An entry of `files:` is either a path, required as before, or
`{ path, optional: true }`.** A required entry that is missing fails the node
exactly as D19 says; the failure's pause offers `retry`, so fixing the cause
does not cost the run.

An optional entry that is missing is not a failed source:

1. The session's context carries an explicit marker in its place —
   `[absent — declared optional; not in the run's tree]` — so the agent knows
   the file is not there and never works from an assumption about it.
2. `context_assembled` names the absent paths (`absent` on the source), so
   replay says exactly what the session saw and the chronicle shows the
   absence as it happens.
3. The materialized object includes the marker, so its hash covers the
   absence like any other content.

A plain entry serializes as the plain string it always was, so no existing
manifest, and no manifest hash, changes.

## Rationale

The absence is declared by the author and recorded by the engine, which is
what I11 asks of every degradation: it is visible in the workflow, in the log
and in the session's own context. Keeping required the default keeps a typo
in a repository's own workflow loud. A pack author who reads a file only as a
shortcut says so where the path is written.

## Rejected alternatives

**Every missing file is a warning and the node continues.** A mistyped path in
a workflow's own repository would degrade every run without stopping one, and
the log would record a session that saw less than its author intended with
nothing to say why.

**A per-source flag (`files: { paths: [...], optional: true }`).** One missing
file would drop the requirement on every file listed with it, and the plain
list form would need a second shape.

**A `requires.files` list in `pack.yaml`.** It repeats what the workflow
already declares, and the two drift apart; the workflow is where a run reads
the path, so it is where optionality is said. `yunta check` and `yunta doctor`
read the workflow to name missing required files before a run starts.
