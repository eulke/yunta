---
number: D262
title: "A reset is a scheduled resume"
status: accepted
revises: [D242]
revised_by: []
---

# D262 — A reset is a scheduled resume

## Context

An account's usage limit can name the moment it resets, and a forge's rate limit
can say how long to wait. Before this decision the engine read both as ordinary
failures with a message. A session stopped by its limit could be retried at once,
against a limit that still held, and the forge's rate limit failed the node
without a wait. A run blocked on a limit whose reset was known could not wait for
it: it either spent attempts before the reset or stopped and waited for a person,
who had the same time to wait for and no better information.

## Decision

1. **A limit with a reset time is a wait.** The run pauses as `awaiting_reset`,
   with the reset as an absolute time, and a detached resume is started for that
   time. The waiting process does not hold the run's worktree. Every `yunta resume`
   waits for the reset before it takes the run's lock or checkout, whoever starts it,
   and a run woken before the reset pauses again on the same reset: the engine itself
   never waits.

2. **The reset is absolute.** A forge's `retry-after` is added to the time the
   failure happened, and a session's reset is the one its CLI reported for the
   window it rejected. A window the CLI allowed names a reset too, but of another
   window, so it is never the limit's reset.

3. **A limit without a reset time is a question for a person.** It is offered as
   `retry` under the precondition that the service accepts requests again. Its
   trade-off says that a retry before then fails the same way.

4. **A reset that is not after the failure is no wait.** A time already past is
   read as a limit without a reset.

## Rationale

The reset is a fact the service stated, and waiting for it costs the run nothing
it would not already spend. A retry before the reset spends an attempt, and the
tokens of its session, to learn what the limit already said. When no time is
given, any wake-up the engine chose would be a guess, and a blind retry would
schedule attempts that cannot succeed, so the choice goes to a person who can read
the trade-off.

## Rejected alternatives

**Retry with a backoff and no reset time.** A backoff that does not know the reset
either retries before the limit lifts, which wastes an attempt, or waits longer
than it must.

**Ask a person about every limit.** A limit with a stated reset needs no decision,
and asking would hold the run for a wait the engine could keep on its own.

**Read an allowed window's reset as the limit's.** An allowed window carries a reset
of its own, so the wait would end at the wrong time.
