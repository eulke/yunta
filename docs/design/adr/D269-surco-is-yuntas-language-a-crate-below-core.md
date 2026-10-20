---
number: D269
title: "Surco, Yunta's language, is a crate below core; render says Yunta's facts with it"
status: accepted
revises: [D133, D221]
revised_by: []
---

# D269 — Surco, Yunta's language, is a crate below core; render says Yunta's facts with it

## Context

D221 gave the words and blocks a reader is told things in a crate of their own,
`yunta-render`, on `yunta-core` alone. D133 left how a problem is laid out in
`yunta_core::text`, and the figures a person reads — a count of tokens, a size,
a ratio, a duration — sat beside it in `yunta_core::units`. So the language
was in two crates, and in the second it shared the crate with Yunta's own
documents: the plan, the receipt, the words of a failure.

Surco's rules are about the language itself: a glyph is written in one place,
an escape in one place, a state word in one place; a block holds what is said
and never a width, an escape or a glyph; every fact a rich terminal draws is in
the words of a plain one. A rule like that holds only where the language is
whole and nothing else is in the crate with it. The surfaces that come next —
color by the terminal's ground, motion that is evidence, an interactive layer,
a page in a browser — need the same home, one that knows no type of Yunta's.

## Decision

1. **`yunta-surco` (lib `surco`) is the language**: the prose grammar (counting,
   ordinals, lists, the nearest name, the problems block), the figures, the
   glyph sets and the marks, the state words, a line and its tones, the paint
   (the one place an escape is written), the look and the width, the generic
   blocks and the document they make, and the surfaces that draw it: a
   terminal and a Markdown file. It depends on no crate of the workspace.
2. **`yunta-core` depends on Surco**: a refusal counts, lists and quotes with
   Surco's grammar. Core keeps the figure a log records, a duration in whole
   milliseconds; how a person reads a figure is Surco's.
3. **`yunta-render` says Yunta's facts with Surco**: the word a node's state is
   called by, what a failure says beyond its row, the plan, the receipt, the
   body of a gate's pull request. A domain fact is never a block of Surco's
   document: it says itself in Surco's blocks, so every surface draws it the
   way it draws any other.
4. **The engine never draws** (D221 §2 stands). The words it answers an agent
   with use Surco's grammar and figures, and nothing else of it.
5. **A golden is Surco's too**: comparing a surface's whole output with the one
   written down for it, and the look of a terminal a golden is rendered for,
   are decided once, behind the `golden` feature every drawing crate's tests
   enable.

## Rationale

One owner per rule. The grammar a refusal is written in and the grammar a
status page is written in are one grammar; the word a listing calls a run and
the word its JSON carries are one word. Below core, Surco is reachable from
every crate that says something — core's refusals, the engine's answers to an
agent, render's documents, the forge's pull request, the command line's pages —
and it can reach none of them, so nothing of Yunta's domain enters the
language.

## Rejected alternatives

**Surco on top of core.** The language would see the domain's types, and the
rule that a block holds no domain type would hold by care, not by the
dependency graph; core's refusals would still need a grammar of their own
below it.

**Surco as a module of render.** The language would share a crate with the
documents it is used for, and every rule that names where something is written
would have to name the exceptions beside it.

**Grammar and figures left in core.** Two crates would decide how a fact is
said, which is what D221 set out to end.
