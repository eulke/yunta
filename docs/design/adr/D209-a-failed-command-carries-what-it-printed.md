---
number: D209
title: "A failed command carries what it printed"
status: accepted
revises: []
revised_by: []
---

# D209 — A failed command carries what it printed

## Context

D193 kept what criteria and hooks print in the run's object store and put
the tail of a failing one on its event. A `bash` node and an executor were
left on the older path: their failure was the sentence `exit N`, followed by
the last lines of **stderr** only, unredacted until the log took the secrets
out, and with no object behind it. A compiler or a test runner says why on
stdout as often as on stderr: a `bash` node running `cargo build` that failed
with `error[E0425] … --> src/main.rs:3:5` on stdout reached every surface —
the live view, the closing block, `status`, the decision a person was asked —
as `exit 101`, while what it printed sat in `node-output/<node>.txt`, a file
no surface names.

A hook's failure already quoted stdout and stderr, but as prose: the node's
failure was a sentence with the hook's tail inside it.

## Decision

1. **A command that exits non-zero fails its node with `exited`**: the exit
   `code`, the `tail` of what it printed — stdout, then stderr, the last
   `TAIL_LINES` lines — and the `output` object holding everything it
   printed, redacted, in the run's object store. When the command was not the
   node's own `run:`, `origin` names it: an `executor` by name, or a `hook`
   by phase and command as the workflow wrote it.
2. **The prose is produced from the data.** The failure reads as its headline
   (`exit 101`, ``executor `probe` exited 2``, ``before hook `x` failed``)
   followed by the tail, which is the sentence every surface already showed
   for a command that printed on stderr.
3. **A decision over such a failure attests it line by line**: the headline,
   then one fact per line of the tail, in the order the command printed them.
4. **`node-output/<node>.txt` stays** for what a `node-output:` context source
   reads; it is not what a person is pointed at.
5. **One constant bounds every tail**: a session's stderr on its death and a
   command's output on its failure keep the same number of lines.

## Rationale

The output of a failed command is the evidence of its verdict, and the person
deciding about the failure is the reader who needs it most. Putting the tail
on the failure itself reaches every surface the failure already reaches —
including the next session's progress file and the decision's record — with no
surface having to know where a file lives. Keeping stdout is what makes it the
reason rather than a fragment of it.

## Rejected alternatives

**Quoting stderr as before and pointing at `node-output`.** The reason is on
stdout for the most common commands a workflow runs, and a path is a second
step every reader has to take.

**Writing the sentence beside the data (`outcome` next to `exited`).** Before
the first published tag a payload is replaced in place; two copies of one fact
on the log are two things to keep in agreement.

**The whole output on the event.** A build log of thousands of lines would
reach every surface that quotes the log; the object store holds it by hash.
