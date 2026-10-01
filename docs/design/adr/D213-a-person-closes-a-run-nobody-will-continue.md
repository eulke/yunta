---
number: D213
title: "A person closes a run nobody will continue"
status: accepted
revises: []
revised_by: []
---

# D213 — A person closes a run nobody will continue

## Context

A run that stops on a person — a decision, a question, a budget — waits for as
long as it takes, and that is the point: nothing about it depends on the
terminal that started it. But nothing ever ended such a wait either. `abort` at
a gate pauses the run, `cancel` on a stopped run answers "already stopped —
nothing to cancel", and no path of the engine writes `run_finished` with
`cancelled`. A run nobody was going to answer stayed under "needs you" for
good: an inbox of runs from days ago, each asking for a person who had already
decided.

## Decision

1. **`yunta close <run>` closes a stopped run as `cancelled`**, writing
   `run_finished { terminal_state: cancelled, closed_by }` with the responder
   who closed it — `--by`, or the ambient `unverified:$USER` — and exporting
   the run's log.
2. **A run whose engine died is settled first**, the way `cancel` settles one:
   what the engine left running is killed and its pause recorded, then it is
   closed.
3. **A run an engine is driving is not closed** — `cancel` stops it — and
   neither is one whose log says it is moving with nothing proving its engine
   gone.
4. **Nothing else of the run is touched**: its branch and its worktree stay for
   a person to read, and for `gc` to remove.
5. **`cancel` keeps its meaning** — stop the work, and the run can resume — and
   `abort` at a gate keeps pausing. A run stopped already is pointed at
   `close`.

## Rationale

A pause is a question to a person, and "nobody will continue this" is an
answer the log had no way to record. Recording it, and who gave it, keeps the
inbox to what still needs someone, and keeps the record of why a run ended.

## Rejected alternatives

**`cancel` closing a stopped run.** One verb would mean "stop the work" on a
running run and "end it for good" on a stopped one; a person who cancels to
resume later would lose the run.

**Folding old runs out of the inbox by age.** The run would stay open and
resumable while looking closed, with nothing on its log saying anybody decided.
