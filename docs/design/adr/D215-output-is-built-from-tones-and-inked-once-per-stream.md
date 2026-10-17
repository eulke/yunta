---
number: D215
title: "Output is built from tones and inked once per stream"
status: accepted
revises: []
revised_by: []
---

# D215 — Output is built from tones and inked once per stream

## Context

The CLI painted one word: `warning`, bold yellow, written as an escape sequence
inside `error.rs`. Everything else was one weight and one color, so a verdict, a
label, a path and a command to copy all read alike. Whether a stream got color
was decided by `NO_COLOR` and whether stderr was a terminal, read wherever it
was needed, and `NO_COLOR` also took the live view away.

## Decision

1. **A line is spans, each in a tone** — what the text is to a reader: plain,
   strong, muted, done, failed, running, needs you, caution. A mark is painted in
   the tone it takes; `needs you` is the one tone that means a person.
2. **One module paints.** It writes the sixteen ANSI colors only, so a terminal
   draws them in its own palette, and it is the one place in the crate that
   writes an SGR escape — held by a counter of the ratchet.
3. **A painted line reads as the plain one with its color taken out**, held by a
   test over every pair of tones.
4. **Color is decided per stream**, from what the process read once at its
   start: `--color always|never`, then `NO_COLOR`, then `CLICOLOR_FORCE`, then
   `CLICOLOR=0`, then whether the stream is a terminal that is not `dumb`.

## Rationale

Color earns its place only by repeating a distinction the words already make,
which is what lets every reader without it — a pipe, a screen reader, a person
who turned it off — lose nothing. Building lines from tones keeps that
distinction in the code that chooses words, and keeps the escapes in one place.

## Rejected alternatives

**Painting at each call site.** Every site would carry its own escape and its
own idea of what a color means, and nothing would hold a painted line to the
plain one.

**A crate for terminal styling.** The sixteen colors and one reset are a few
lines; a dependency would bring its own policy for when to paint.
