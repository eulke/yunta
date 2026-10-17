---
number: D212
title: "A run records the repository it was created in"
status: accepted
revises: []
revised_by: []
---

# D212 — A run records the repository it was created in

## Context

Every run on a machine lives under one state root (`~/.yunta/runs`) and one
database, whichever repository it was started from. Nothing a run froze said
which repository that was: the manifest carries the workflow, the config, the
base branch and commit, and the state roots, but no identity of the checkout.
`yunta list --runs` therefore listed, inside one project, the runs of every
other project on the machine, unlabelled.

## Decision

1. **The manifest records `project: { git_common_dir }`**: the git directory a
   repository's main checkout and every one of its linked worktrees share,
   absolute and canonical. It is read once, when the manifest is frozen, from
   the checkout the run is created in.
2. **It is optional.** A manifest written before it, or frozen where git could
   not answer, carries none; the manifest's schema goes to 3 and every older
   manifest reads as it did.
3. **A run without one is placed by its branch**: a run whose own branch
   (`yunta/run/<id>`) exists in a repository belongs to it. All of a
   repository's run branches are read in one listing, never one call per run.

## Rationale

The common git directory is the one path every checkout of a repository agrees
on, linked worktrees included, and it does not depend on where the person
typed the command. Freezing it with the rest of the manifest makes it a fact
of the run rather than a guess a later reader makes.

## Rejected alternatives

**Matching by base commit.** Forks and clones share commits, so two projects
would claim each other's runs.

**The project name from the config.** It is optional, it can be set in the
user layer, and two checkouts of one repository can name themselves the same.
