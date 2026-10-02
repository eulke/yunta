---
number: D220
title: "The run list names the ten newest closed runs and counts the rest"
status: accepted
revises: [D170]
revised_by: []
---

# D220 — The run list names the ten newest closed runs and counts the rest

## Context

`yunta list --runs` is read for what needs someone. In a repository with a
history, every run that ever closed was listed under the ones still open, two
lines each, and the listing scrolled the runs a person had to act on off the top
of the screen. A closed run asks nothing of anyone; the newest are the ones a
reader comes back to look at.

## Decision

1. **Closed runs are listed newest first**, the other way round from the groups
   a run is still in, where the one that has waited longest comes first.
2. **Only the ten newest closed runs are named.** The rest are counted on a
   line of their own: `2 older closed runs not listed`.
3. **A run that needs someone carries what holds it and the command that moves
   it** under its row, the reason said whole rather than cut to the line.

Ten is registered in D170 as `CLOSED_SHOWN`.

## Rationale

The heading of each group already counts every run in it, so folding closed runs
loses no count; `yunta status <run>` reads any one of them by its handle. Ten is
enough to find the run that just finished among the ones before it, and few
enough that the listing still opens on what is waiting.

## Rejected alternatives

**Every closed run, as before.** The groups above it are the reason the listing
exists, and they scrolled away.

**No closed runs at all.** The run that finished a minute ago is the one a
reader most often comes to look up.
