---
number: D190
title: "`yunta check` refuses every stop the run's frozen inputs make certain; a warning is for what the run may still get through"
status: accepted
revises: []
revised_by: []
---

# D190 — `yunta check` refuses every stop the run's frozen inputs make certain; a warning is for what the run may still get through

## Context

A run of `yunta/fragua` in quick mode spent about 12M input tokens on
`grill`, `brief`, `plan`, `implement`, `lint` and `fix-lint`, then failed at
`tests` in no time: a `baseline_compare` node, in a project whose config
declared no `baseline.suite`. The lineage had measured nothing before its
first node, so the comparison had nothing to compare against. The config
was frozen in the run's manifest, so a `retry` could not change it, and the
failure's menu offered one anyway.

All of it was known before the first token. `yunta check` warned about the
opposite case, a suite nothing compares against, and said nothing about
this one. Workflows built to fail the same way passed `yunta check` and
failed at run time: a `coverage_gate` with no `coverage:`, an executor no
`skills.executors` entry registers, a session with no runner and no
`defaults.runner`, a loop with nothing to give it a tasks document, a
context artifact read from a node that does not produce it, an external gate
publishing an artifact nobody produces, a `node-output:` of a node that
captures none. A workflow composed with `use:` was checked only when its
child was born, halfway through the parent's run. A `files:` path was a
warning even when nothing before its reader could write it. A pack's
`requires:` was read only by `yunta doctor`.

D34 said check errors come before a token is spent; D76 said a warning is
for what is genuinely doubtful; D100 said an error only when it is provable.
Each settled one case, and nothing said which side a new rule belongs on.

## Decision

1. **The criterion.** A run freezes its workflow and its config when it is
   created, and starts from one commit. Whatever those — and what a pack
   declares it requires — make certain to stop the run is a `check` error:
   the run could not change it, so finding it late only costs what ran
   before. Whatever the run may still get through — a risk, a waste, a case
   the author may be right about, or one only the run can settle — is a
   warning, and it says what would make it certain. A rule that cannot tell
   which case it is looking at warns.
2. **What the criterion refuses today.** A config key a node's kind cannot
   run without (`coverage`, an executor's registration, a runner); a
   comparison, anywhere in the composition, under a config with no
   `baseline.suite`; a read nothing in the run can answer, in any mode the
   workflow declares — a loop's tasks document, an artifact a context
   source, a mount or an external gate names, a `node-output:` of a node
   that is not `kind: bash`; a mode that keeps a node and leaves out a node
   it reads from, when no mode before it keeps that node either; a
   composed workflow its birth would refuse, checked with
   what its node mounts into it; a `files:` path nothing that can run
   before its reader can write; what a pack requires and the project or
   machine lacks; a declaration no candidate adapter can honor, the run
   tools included.
3. **What it warns about.** The existing warnings stay warnings, and two
   join them. A literal command whose program is not on `PATH`: a node
   before it may install it, and a shell script read without a shell is
   read by heuristic. A read a mode answers only with what an earlier
   mode made: a run promoted into the mode is born holding it, so the
   workflow may well be meant that way — but `yunta run --mode` starts
   the mode fresh, holding nothing, and refuses it there.
4. **Runtime says the same.** A run that meets an unset key anyway — born
   under an older binary — fails the node with `unset`, in the same words
   `check` uses, and the failure's menu offers no `retry`: every attempt
   reads the same frozen config. It names the way out, a new run under a
   config that declares the key.

## Rationale

The cost of a certain stop grows with every node before it. Refusing it at
`check` is free; meeting it at the node costs the run, and a `retry` offered
there is a question with one honest answer. Stating the rule once lets the
next check be placed without a decision of its own.

What a run reads at `check` time and at run time is the same function
wherever the rule allows it: a config key is `ConfigKey::unset` in both
places, and the run tools a node needs is one predicate. A rule the run
enforces on its own can drift from the one `check` enforces; one both
call cannot.

## Rejected alternatives

**Severity as a field on each finding.** Every caller that treats `check`'s
errors as "must be empty to proceed" would have to filter, and a finding
with a wrong severity would still start the run.

**Warning when unsure whether a reader's file can be written.** The files
case is refused only when nothing that can run before the reader could
write the path; any doubt keeps the warning, so a workflow that writes its
own inputs is never refused.

**Keeping `retry` on a frozen-config failure, with a warning in its
tradeoff.** A choice that cannot change the outcome is not a choice; the
menu keeps only what can.
