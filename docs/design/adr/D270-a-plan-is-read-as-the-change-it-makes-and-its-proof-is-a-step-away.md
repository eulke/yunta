---
number: D270
title: "A plan is read as the change it makes, and its proof is a step away"
status: revised
revises: [D194, D223]
revised_by: [D271]
---

# D270 — A plan is read as the change it makes, and its proof is a step away

*(Revised by D271: point 2's card also says, between what a task keeps and what tests it, what each shape its file already holds implies, and every code block says under it its size and, for a shape the engine read, what the code is to the tree.)*

## Context

D223 drew a plan whole on read, "with the spec's tests on their tasks": each
task's card carried the code of every test the spec wrote for it, and a review
cut each file at sixty lines. Read where a person decides on a plan, the proof
outweighed the change. In a plan of four tasks, the four spec files and their
captions took more of the page than everything the plan said it would change,
and each came before the change of the task after it. The title counted the
plan's tests before its summary said what it does; every task said how it is
checked twice, the plan's criteria and the spec's tests, each with its command;
and a shape the planner repeated as a change's code was drawn twice.

A person approving a plan decides on a change: what it does, how it works,
what it decides for them and what it risks. How the work is tested decides
only when a test is missing or proves nothing the spec wrote.

## Decision

1. **A plan opens with what it is and what the person asked for.** Its title
   says how big it is, what it decided for them, what it risks and leaves out;
   under it come its summary, what keeps it from being proven when anything
   does, and what the person asked for. Where it stands for its reader is the
   page's to say, never the plan's: a file it is drawn into carries no word of
   a decision that is answered later. The request is the input the workflow
   declares as the person's (`request: true` on a `string` input, one input at
   most), quoted as it was given. Every departure from the plan a person
   accepted is read right after its head, where the work stops being the plan.
2. **Then the change, the steps, and each task's card.** The description is
   read whole, every diagram in it drawn; each choice says what it chose
   instead of what, and why. The steps are drawn from what each task waits on.
   A card says what a person sees once the task is done, the files it changes,
   the shapes of another task it builds on, what it keeps and what tests it —
   each test the spec gives it, in the spec's words, which stand for the
   plan's own criteria when a spec covers the task — then the code the task
   writes: each shape and each change once, a shape under the change that is
   about it. This revises D194's third point: a card names the files a task
   changes, not its scope, and the order its tasks wait in is drawn with the
   steps rather than said on each card.
3. **The proof is a section of its own, and the last.** It counts the spec's
   tests and, where a review leaves them out, names the command that shows
   them whole; says the suite every task keeps passing once; says each guard
   that checks what a task keeps; and counts the promises no guard checks.
   Where a decision is put beside the plan, what keeps it from being proven is
   said beside the decision; the whole plan says it in its proof.
4. **No test's code is part of the read.** A review shows none; the whole plan
   draws each file the spec wrote after the cards, with the task its tests
   prove. This revises D223's third point: a plan is still drawn on read as the
   run judges it, and its spec's tests are read where its proof is. The command
   names the node whose page shows the spec whole: the one that wrote it, or
   the planner when the spec came from no node of the run's.

## Rationale

The order is the order of the decision: what was asked, what changes, how, and
whether it can be shown done. A test is part of that answer as a count and a
command, and its code is one step away for whoever wants it. Saying the proof
once, in a section rather than on every card, keeps one fact in one place, and
a flaw that decides stays in view beside the decision it withholds.

## Rejected alternatives

**Fold the test code inside each card.** A fold a terminal cannot open is a
promise the page does not keep, and the code would still sit between the
changes of every task.

**Keep the tests on their tasks and shorten them.** A shorter test is still the
proof before the change, and a cut file is a file nobody can run.
