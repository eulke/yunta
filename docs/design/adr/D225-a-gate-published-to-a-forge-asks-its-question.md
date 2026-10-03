---
number: D225
title: "A gate published to a forge asks its question, says how a review answers it, and asks again after changes"
status: revised
revises: [D66]
revised_by: [D227]
---

# D225 — A gate published to a forge asks its question, says how a review answers it, and asks again after changes

## Context

An external gate opened a pull request titled `yunta: node `x` is waiting on
external review (assignee: y)`, with that sentence as its body and the run's
marker under it. The question the author wrote in `message:` never reached the
forge, nor did what a review does to the run: that approving lets the run go on,
that requesting changes sends the comments to the node `on_failure` names, that
closing ends the gate. The artifacts were published as the run holds them — a plan
as its YAML — which a reviewer reads with more effort than the document a person
deciding at a terminal is shown.

A request for changes also could not be answered. The gate failed, its re-route
ran, and the gate came back to poll the pull request it had already published: the
same review was still the last decisive one, so the gate failed again on it, lap
after lap, until its re-routes ran out. The corrected artifacts never reached the
pull request, because publishing reused an open pull request without committing
anything to it.

## Decision

1. **The pull request asks the gate's question.** Its title is the gate's
   `message:` (or what the gate needs, when it has none). Its body says which gate
   of which run waits on it, how a review answers it — approve or merge, request
   changes, close — and what each answer does to the run, the files it carries,
   and the commands the machine that holds the run takes next.
2. **The engine hands the decision as data.** The publish request carries the
   question, the assignee, the nodes that run once the gate passes, the node a
   request for changes is sent to, and the documents published for it; the forge
   adapter draws the body as Markdown.
3. **A document a person reads is published drawn, beside its bytes.** A plan is
   published as `tasks.md` — the same whole document `yunta status --node` prints,
   with the spec's tests on their tasks — next to the `tasks.yaml` the run holds.
4. **A request for changes closes that round.** The gate forgets the handle it
   polled, so its corrected lap publishes again: the artifacts are committed to the
   same branch — a file whose content is already there is left alone — and the
   open pull request is reused.
5. **A request for changes decides only what it reviewed.** Like an approval, it
   counts when it covers the pull request's current head; one left on an earlier
   head is waiting, so the corrected work is reviewed before the gate fails again.

## Rationale

A reviewer on the forge has no terminal and no `yunta`: what the pull request
says is all they have to decide with, so it says what a decision prompt says. The
answers a forge records are fixed by the forge, so the body names those rather
than the options a gate declares for a terminal, and the consequences come from
the workflow — the engine knows them, the adapter draws them. Re-publishing on the
same branch keeps one pull request per gate across laps, with its history.

## Rejected alternatives

**Put the plan in the pull request's body.** A body has a size limit and a plan
with its code reaches it; a file is read in the forge's own viewer and diffs
between laps.

**Close the pull request after a request for changes and open another.** The
conversation a reviewer had is on the first one, and the next lap's diff is the
answer to it.
