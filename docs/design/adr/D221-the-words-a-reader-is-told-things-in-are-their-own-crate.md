---
number: D221
title: "The words and blocks a reader is told things in are their own crate, and the engine hands it data"
status: accepted
revises: []
revised_by: []
---

# D221 — The words and blocks a reader is told things in are their own crate, and the engine hands it data

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
   marks, glyph sets, inks, widths, the blocks, and how a plan, a spec,
   findings and a receipt are drawn. It depends on `yunta-core` alone, so every
   edge that shows Yunta to a reader — the command line, an adapter that writes
   a pull request — draws with it.
2. **The engine never draws.** It derives data — a receipt, a plan as the run
   judges it, a decision — as core types, and hands them to whoever shows them.
   Deciding how a fact looks is presentation, and the engine's job is the run.
3. **What depends on the process stays with the command line**: the color
   policy, the width and the glyph set settled once by `main`, which stream is a
   terminal and how wide it is, and how a run's word becomes an exit code. The
   crate takes these as arguments.
4. **What adapts the engine's types stays with the command line too**: the word
   of a `RunFrame`, where a node of a frame stands, the counter line. They are
   functions, not methods, because the crate never sees those types.
5. **What a reader is shown is a core type** (`ShownDocument`, `Receipt`), since
   the engine derives it from a run and the crate draws it.

## Rationale

Two readers told one fact in two words read them side by side: a person sees
`finished` in `status` and the receipt says `Done`. One crate is the one place
those words come from. The engine could reach it too — nothing in the crate
knows the engine — but then the engine would decide how a document looks, and
a run's executor would change every time a medium did. Keeping the engine on
data puts each medium at its own edge: the terminal at the command line, a
pull request's page at the forge adapter.

## Rejected alternatives

**Words in core, blocks in the command line.** The forge adapter, which writes
the body of a pull request, sits below the command line and could not draw a
block; it would lay out its own.

**The engine draws the files and pull requests it writes.** It would own
presentation beside the run, and a new medium would reach into the executor.

**A presentation port the command line implements for the engine.** A pull
request a node opens is drawn by the forge adapter that opens it, which is
already an edge; a port would only route the same call through the engine.
