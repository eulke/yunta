---
number: D226
title: "Every commit a run makes names its run, node and task in git trailers"
status: accepted
revises: []
revised_by: []
---

# D226 — Every commit a run makes names its run, node and task in git trailers

## Context

A run commits in seven places: what a node leaves in the run's tree and what it
finds there, the tree a node's own checkout starts from and what it lands, a
task's integrated work and what a blocked attempt left, and the knowledge it
distills. Their subjects say `node build: …` or `task T001: …`, but nothing in a
commit said which run made it. A branch pushed, merged or rebased is read without
the run's log, and two runs of one workflow write the same subjects.

## Decision

1. **Each commit a run makes ends in trailers:** `Yunta-Run: <run id>` always,
   `Yunta-Node: <node>` when a node's work made it, `Yunta-Task: <task>` when a
   task's did. The run is named by its full id, which is what a record keeps.
2. **One builder writes every message**, in core, so the engine's commits and the
   ones a forge adapter makes for a published gate say it the same way.
3. **Subjects do not change.** They are what a person reads in `git log --oneline`,
   and the trailers carry what a tool reads.
4. **Which commits run hooks is stated, not hidden.** What a node leaves in the
   run's tree is committed with plumbing, which runs no hook, so the commit holds the
   tree the log names. A checkout of a node or task's own, and distilling, commit
   with `git commit`, which runs the project's hooks.

## Rationale

Trailers are git's own format for facts about a commit: `git log --grep`,
`git interpret-trailers` and forges read them, and they survive a rebase or a
cherry-pick that keeps the message. A commit is the one record of a run that
leaves the machine with every push.

## Rejected alternatives

**The run id in the subject.** Every subject would carry 26 characters a reader
skips, and a subject is cut to fit a line where a trailer never is.

**A git note per commit.** Notes are not pushed or fetched by default, so a clone
would not have them.
