---
number: D221
title: "The words and blocks a reader is told things in are their own crate, below the engine"
status: accepted
revises: []
revised_by: []
---

# D221 — The words and blocks a reader is told things in are their own crate, below the engine

## Context

Everything a person reads on a terminal — the words a run and a node are called
by, the marks beside them, the blocks a status page, a closing block or a gate
is built from — lived in the command line's `render` module. The engine writes
for people too: the plan view a gate points to, a receipt, the body of a pull
request. It could not reach those words and blocks, because the command line
sits above it, so it wrote its own: a receipt that said `state: Done` and
crossed out what it had nothing to prove, a plan view that named the same facts
with other labels.

An audit of the module found that no block takes a type of the engine's. What
holds it to the command line is the process: which stream is a terminal, how
wide it measures, what `NO_COLOR`, `COLUMNS` and `YUNTA_GLYPHS` asked for.

## Decision

1. **`yunta-render` holds the vocabulary and the blocks**: run and state words,
   marks, glyph sets, inks, widths, the blocks, and how a plan, a spec and
   findings are drawn. It depends on `yunta-core` alone, so the engine and the
   command line both draw with it.
2. **What depends on the process stays with the command line**: the color
   policy, the width and the glyph set settled once by `main`, which stream is a
   terminal and how wide it is, and how a run's word becomes an exit code. The
   crate takes these as arguments.
3. **What adapts the engine's types stays with the command line too**: the word
   of a `RunFrame`, where a node of a frame stands, the counter line. They are
   functions, not methods, because the crate never sees those types.
4. **What an escalation shows is a core type** (`ShownDocument`), since the
   engine reads it from a run and the crate draws it.

## Rationale

Two readers told one fact in two words read them side by side: a person sees
`finished` in `status` and the receipt says `Done`. One crate is the one place
those words come from. Putting it below the engine, rather than putting the
engine's files behind the command line, keeps the engine the owner of what it
writes and lets any reader the engine serves — a forge, a file — be drawn the
same way.

## Rejected alternatives

**Words in core, blocks in the command line.** The engine would share the words
and still lay out its own blocks; the receipt and the plan view are blocks.

**The command line writes the engine's files.** A pull request a node opens
mid-run, and the plan view written when a plan is accepted, happen inside the
engine, with no command line to call.
