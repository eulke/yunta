---
number: D194
title: "A gate shows what it asks about, and a correction goes back to the session that made the work"
status: accepted
revises: []
revised_by: []
---

# D194 — A gate shows what it asks about, and a correction goes back to the session that made the work

## Context

Fragua's `approve-plan` gate asked "Plan registered. Approve?" with the assignee as
its only evidence. A person who wanted to validate the plan had to find the tasks
document on disk, and nothing on the log said which version they approved.

Choosing `adjust` re-routed to `plan`. The console already asked "anything to
add?" and kept the answer in `gate_resolved.free_text`, but nothing carried it to
`plan`: its next attempt opened a fresh session on the same brief, explored the
repository again, and planned without the correction. With no words given, it
planned again with nothing changed.

Modes already decide whether the review happens at all: `quick` leaves the gate
out, `standard` keeps it.

## Decision

1. **An internal gate declares what it shows.** `shows:` takes the references a
   `context: artifact` source takes, creates the same implicit `depends_on`, and
   is held to the same rules by one list of a node's artifact reads. An external
   gate refuses it: the forge shows its `artifacts:`.
2. **The escalation names what it shows by hash.** `gate_waiting.shows` records
   the producer, identity and content hash of each document as the run held it,
   so an approval is an approval of those bytes. A parked run rebuilds the same
   list from the log.
3. **The surface presents the documents.** The engine reads them from the object
   store and hands them to the surface with the question. A tasks document is
   drawn task by task with its scope, criteria and dependencies; any other
   document, by its text; both with the path to the whole file.
4. **An option that sends the run back to a session asks what should change.**
   `GateOption.asks` is the question; an answer to that option without words is
   refused on every surface, because re-running without them would change
   nothing.
5. **The words go back to the session that did the work.** The node's next
   attempt resumes its last session, told the review. A fresh session, when that
   one cannot be resumed, gets the brief, the review and the paths of what the
   node handed over before.

## Rationale

A person approving a plan is judging a document, so the document belongs in the
decision, and the record of the decision names it. A correction is the one thing
that changed for the planner, like the answer to a scope request. Resuming its
session spends a message and the turns the revision needs, instead of a second
exploration of the repository.

## Rejected alternatives

**An auto-approve setting.** A gate that answers itself records a decision nobody
made. Leaving the gate out through a mode records what happened: this run did not
review the plan.

**Copying the plan into the escalation.** The document is already in the object
store by hash. A copy on the log would be a second version of it.

**Optional words for a correction.** An empty correction would re-plan blind.
