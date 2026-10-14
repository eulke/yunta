---
number: D255
title: "What a session writes is formatted the moment it writes it"
status: revised
revises: []
revised_by: [D266]
---

# D255 — What a session writes is formatted the moment it writes it

*(Revised by D266: decisions 2 to 4 are revised in part. The formatters of one write share one bound of 5 seconds, a formatter still running at it is stopped and recorded with `timed_out`, `write_formatted` carries the redacted output the engine kept, and `stats` reads the p50 and p95 of each moment from each session's `rail_summary`.)*

## Context

A run's formatting failures surfaced late: a `fmt` guard or a lint node
red at the end, and a session with none of the context of the one that
wrote the file sent to fix it. Formatting is mechanical — the project's own
formatter decides it — and a session that never has to think about it never
leaves anything for a check to find. Both CLIs run a hook after a write and
read `additionalContext` from it; a file changed under a session makes its
next edit on stale context fail once, after which it reads the file again.

## Decision

1. **`format:` in the config names a project command per set of files.**
   The first rule whose globs match a file formats it; its command is one
   `commands:` declares, and `check` refuses one it does not.
2. **The engine formats on the rail's `after-write`.** Every file one write
   names that the session may write and a rule covers is handed to that
   rule's command as an argument; the engine compares the files before and
   after and records `write_formatted`, with how long the session waited.
3. **The session hears only what it must.** When the command changed a
   file, the session is told to read it again before editing it; when it
   failed, what it printed. Otherwise nothing is said.
4. **The hook is installed only when the run formats**, and its time is
   `stats`' own `rails` share, taken out of `working`.

## Rejected alternatives

**Format at the close.** It formats after the session can learn anything
from it, and a check before the close still sees the unformatted file.

**Tell the session the project's style instead.** It spends tokens on
what a program decides exactly.
