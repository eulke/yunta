---
number: D214
title: "One vocabulary, one mark per meaning, and exit codes that say which word"
status: accepted
revises: []
revised_by: []
---

# D214 — One vocabulary, one mark per meaning, and exit codes that say which word

## Context

A node's state had two vocabularies: the short one a column used (`done`,
`fail`, `run`, `wait`, `skip`, `todo`) and the long one a sentence used
(`finished`, `failed`, `running`, `waiting`, `skipped`, `never ran`). The short
`run` collided with the noun every surface uses for a run: `● run fix`,
`nodes 0/5 · 1 run`. A run stopped on a person was `paused`, a word that says
nothing about who has to act, while the inbox heading over it already said
`needs you`.

The mark `◆`, which says "a person", also marked a re-route, a red baseline, an
event kind the binary did not know, a promoted child and a run that finished
holding blocking findings: a reader who learned the mark learned nothing.

Every stop that was not a clean finish exited 1, the same code as a command
that could not start: a script could not tell "needs a person" from "broke".

## Decision

1. **One vocabulary for a node's state**, the long words, in a column and in a
   sentence alike — they are the words `yunta test` cases are already written
   in. The column is as wide as the widest word.
2. **A run's words** are `created`, `running`, `stalled`, `needs you`,
   `finished`, `reported`, `failed`, `cancelled`, `promoted`, `broken`, on every
   surface and in the JSON. `reported` is a run that finished holding blocking
   findings.
3. **A mark means one thing.** `◆` is a person, and only a person; a re-route and
   a promotion are `↻` (`~`); a caution — a run nobody drives, findings that
   block, a red baseline, an unknown event kind — is `▲` (`!`).
4. **`run` and `resume` exit with the word**: 0 finished, 3 needs you, 4
   reported, 1 any other stop or a failed command, 130 an invocation a person
   interrupted; clap's 2 stays for a command line that did not parse. `status`
   and `close` exit 0.
5. **The JSON documents go to `schema_version: 6`**, since `outcome` changes
   meaning for two words. A `yunta test` case that writes `paused` is refused,
   naming `needs you`.

## Rationale

A word a reader acts on has to mean the same thing everywhere, and a mark is a
promise the eye relies on before it reads. An exit code is the word a script
reads; giving it the same distinctions a person gets is what lets CI tell
"waiting on a reviewer" from "broken".

## Rejected alternatives

**Keeping the short words for columns.** Two words for one state is the drift
the vocabulary exists to prevent, and the short set is the one that collided.

**`paused` beside `needs you`.** A run stopped on a person is one state; two
names for it is the inbox and the page disagreeing.
