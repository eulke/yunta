# Working in Yunta

Yunta executes YAML workflows. Declared criteria determine completion; the
event log records what happened, and replay derives state. Engine decisions
are deterministic and effects have explicit owners.

## Reason from an observable claim

Frame a task as a change in observable behavior. What happens now, what should
happen, and which assumption might be wrong? Read nearby code and tests.
Consult a relevant contract or prior decision when intent matters; check
current primary documentation for external APIs. When sources disagree,
explain the discrepancy. Separate facts, hypotheses, and choices.

## Let the domain shape the code

Choose data structures that reflect valid domain states before adding
branches. Find the owner of each invariant and trace how data enters,
persists, is replayed, and reaches a caller. A small interface that hides
real complexity is easier to use than scattered special cases. Consider the
design you would choose if the requirement had always existed; reach it in
coherent increments and remove obsolete paths. An abstraction earns its
place by reducing duplicated rules, invalid states, or caller effort.

## Challenge the claim

Evidence should be able to disprove the solution. Reproduce bugs and explain
their cause; show that refactors preserve public behavior; compare performance
against a baseline. Tests assert independently known outcomes. Match checks
to the claim and report their actual results.

## Keep knowledge close and small

Encode recurring corrections in types, checks, or shared code. Record a
decision for a surprising tradeoff that is costly to reverse and cannot be
explained by the code.
Update an existing reference before adding a file; keep temporary
context in the task summary. Use English for documentation, comments, UI text,
and examples.
[CONTRIBUTING.md](CONTRIBUTING.md) has local commands; [the design
index](docs/design/README.md) points to contracts and decisions when needed.
