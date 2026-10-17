---
number: D265
title: "A breach is filed where the recorded confinement leaves the checkout to the fence alone"
status: accepted
revises: [D172]
revised_by: []
---

# D265 — A breach is filed where the recorded confinement leaves the checkout to the fence alone

## Context

D172 files a write that reaches the diff as `engine-fence-breach` when the
session's coverage is `Exact`: the adapter declared an exact fence, and a write
crossed it. Every confined session now names the temporary directories among
its roots, because `$TMPDIR` and `/tmp` are writable in every session. Its
coverage is therefore `WidenedToRoots` and never `Exact`, so that rule files
nothing for a confined session, including one whose file tools were the only
writers that could reach the checkout.

## Decision

1. **A breach is a write that only the fence could have stopped.** The engine
   files one when the session's recorded confinement leaves the checkout to the
   fence alone: its commands are confined and it writes nothing of the checkout
   (`worktree: nothing`). That is a `read-only` profile, or an `edit` profile
   under the `read-only` sandbox.
2. **Every other violation is only a violation.** The node fails with the list of
   paths, and nothing is filed against the adapter. A session whose commands can
   write the checkout, an `edit` session under the `workspace-write` sandbox or
   any unconfined session, may have written it through a command.
3. **The record is what the rule reads.** `agent_session_opened.confinement`
   carries the confinement. A log written before that record has none, and the
   rule reads the coverage instead, where `Exact` marks a breach as it did before.
4. **This revises D172.** Its item 7 names `Exact` coverage as the signal. The
   coverage no longer carries that signal; the confinement, which the engine
   resolves once per session, does. The rest of D172 stands.

## Rationale

The finding says the adapter failed to fence a write it claimed to fence. That
holds only when the fence is the one writer that could have reached the path. A
command under `workspace-write` can write the checkout, so a write it makes is
not a failure of the fence. When the commands are confined and write nothing of
the checkout, only the file tools can write there, and the fence judged each of
their writes before it happened: a write that still reaches the diff is a
failure of the fence itself. The rule reads the recorded confinement, not the
coverage, because the record says which of the two cases a session was in.

## Rejected alternatives

**Keep reading the coverage.** Every confined session names the temporary
directories, so `Exact` is never reached and the finding is never filed.

**Remove the temporary directories from the coverage.** The coverage would then
misstate what the commands can write, since the sandbox grants them.

**File a breach for every violation.** A command under `workspace-write` may
write the checkout, which its sandbox allows, so the adapter would answer for a
write it did not fail to fence.
