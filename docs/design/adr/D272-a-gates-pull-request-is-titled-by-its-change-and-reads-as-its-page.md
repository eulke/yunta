---
number: D272
title: "A gate's pull request is titled by the change it asks about and reads as the gate's page within the forge's limit, its files go in a folder of the run's, and closing it pauses the run"
status: accepted
revises: [D225, D270]
revised_by: []
---

# D272 — A gate's pull request is titled by the change it asks about and reads as the gate's page within the forge's limit, its files go in a folder of the run's, and closing it pauses the run

## Context

A gate asked its author's `message:`, which no workflow can template, so fragua's
plan gate asked "Plan and its tests registered. Approve?" and never named what it
would approve. A gate's pull request was titled with that message. D225 kept the
plan out of the description because a description has a size limit and a plan with
its code reaches it: the description said which gate waited, how a review answers
it, the files it carried and two commands, under a heading followed by the title of
a list of next steps, and the plan was a file at the branch's root, where a
project's own `tasks.md` is written over. A reused pull request kept the
description it opened with, so after a correction it described a plan the branch no
longer held.

Closing the pull request failed the gate, while `abort` at a terminal pauses the run
with the decision open: two answers that mean "not now" did different things, and
with `on_failure` the re-route came back to poll the same closed pull request until
its re-routes ran out. The description offered `yunta cancel`, which stops nothing on
a run already paused, and `abort` read "Abort the run" while it pauses.

At a terminal the gate's page said its question under a second headline, its
options twice, and a plan's risks twice. GitHub refuses a description over 65536
characters and a title over 256; it documents neither.

## Decision

1. **A gate's question names the change.** A gate with no `message:`, or a blank one,
   that shows a plan whose summary says what it changes asks "Approve the plan: <summary>?". The
   log keeps it as data, `approve_plan` with the summary; every surface writes its
   words. An author's `message:` is still the question as written.
2. **`abort` reads "Pause here"**, its id unchanged, with "nothing runs; this
   decision stays open", on every menu where choosing it pauses the run.
3. **A gate's page is one page wherever it is drawn**: one headline, each document
   it shows as a review reads it, then the decision under its question, what weighs
   on it and each option with what it does. Where a decision is put beside a plan,
   the plan's risks are said there, once, as what keeps it from being proven already
   is (D270). At a terminal no option names a command: the prompt under the page is
   how it is chosen, and the commands are said where the run is parked.
4. **The pull request is titled by the change**, "Approve: <summary>" when it
   publishes a plan that says one and its question otherwise, **and its description
   is the gate's page** in the forge's Markdown: sections are headings of the second
   level under the forge's title, a task's card and the proof are folded, the risks
   are an alert, the steps' order is Mermaid, and the decision is a table of the
   answers a review gives, what to do on the forge and what each does to the run,
   with the commands of the machine that holds the run after it under one title. On
   a forge's page the proof names no command to show the spec's tests: the whole
   plan is published beside it. Every lap draws the title and the description again.
5. **The description fits the forge's limit.** Measured in bytes of UTF-8, never
   fewer than the characters the forge counts, it leaves out, in this order, each
   code block's lines, what each fold holds, the documents altogether, and what
   weighs on the decision, and says at its top what it left out and where it is
   whole. The decision and every answer always stay: a question too long for the room
   left is cut, and a description that does not fit even then is refused before
   anything is committed. A title past the limit is cut.
6. **A gate's files go in a folder of the run's**, `.yunta/gates/<run>/<gate>/` on
   its branch, which its publication records; a gate whose log published at the
   branch's root keeps publishing there for the rest of its run.
7. **Closing the pull request pauses the run as `abort` does.** The log records the
   resolution and the pause and nothing of the node; the gate forgets the pull
   request, the run reads stopped, and a resume asks again, on the pull request a
   person reopened or on a new one. A log where a closed pull request failed its
   gate reads as it was written.

## Rationale

A reviewer decides on what the pull request says, and it says its title and its
description first. The gate's page is already what a person decides on at a
terminal; drawing it on the forge keeps one wording and one order, and the forge's
own constructs fold what a reader opens on demand. Code is the bulk of a plan and
reads as well in the file viewer, so it is what leaves first, in an order a reader
can predict. A close is a reviewer saying "not now", which is what `abort` means. A
folder per run and gate leaves the project's files and other gates' where they are,
and recording it keeps a gate in flight writing where its reviewers read.

## Rejected alternatives

**Keep the plan out of the description.** The page a reviewer opens said nothing of
the change; the limit is met by leaving code out first.

**Cut the description at the limit.** The decision is its last part and would be
what is cut; a cut inside a fence or a table breaks what follows.

**Put the decision first on the forge.** Two arrangements of one page drift apart;
the title already says what is asked, and the answers sit at the end, beside the
forge's own controls.

**Template `message:` with the summary.** A message would read an artifact the gate
does not name, and every author would write the same sentence.

**A command beside each answer on the forge.** A published gate is answered on its
forge; the one command that moves the run after any answer is said once.

**Fail the gate when its pull request closes.** Two stops would mean different
things, and a re-route would poll the closed pull request again.

**Leave the folder out of the log.** A gate published at the branch's root before
gates had a folder would write its next lap into the folder, and its pull request
would carry two plans.
