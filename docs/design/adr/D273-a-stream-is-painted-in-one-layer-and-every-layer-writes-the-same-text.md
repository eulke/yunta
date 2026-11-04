---
number: D273
title: "A stream is painted in one layer, plain, sixteen colors or 24 bits tuned to its ground, and every layer writes the same text"
status: accepted
revises: [D215]
revised_by: []
---

# D273 — A stream is painted in one layer, plain, sixteen colors or 24 bits tuned to its ground, and every layer writes the same text

## Context

D215 painted the sixteen ANSI colors and nothing else. Sixteen colors draw a
word in a role, and no background a theme agrees on, so nothing could stand
apart from the page as a region: the decision that waits on a person, a block
of code, the words a change adds and removes, the word that says where a run
stands. Most terminals in use draw 24-bit color and say so in `COLORTERM`.
Links were a third kind of ink beside plain and color, though a link belongs to
any stream with color.

Two places drew different words depending on color. A code span in Markdown
lost its backticks where a stream painted, since its tone set it apart, and a
run's furrow was a rule in plain ASCII and strokes in color. D215's test, over
pairs of tones, could not see either.

## Decision

1. **A stream is painted in one layer**: plain, the sixteen ANSI colors, or
   24-bit colors tuned to a dark ground or a light one. Whether a path is a link
   is a property of a layer with color, never a layer of its own.
2. **A span or a line may carry a fill**: the tint of a decision that waits, the
   ground of code, the ground of a word a change adds or removes, and a pill for
   a word set in its own color. Only the 24-bit layer paints fills; the sixteen
   colors paint each tone as D215 decided and no fill. A filled line reaches the
   edge of the terminal by erasing to it with its background set, never by
   writing spaces.
3. **Every layer writes the same text.** A painted line with its escapes taken
   out reads exactly as the plain one, in Unicode and in ASCII, which a test
   holds by drawing every block of the catalog in every layer. Nothing a surface
   writes depends on the layer: a code span keeps its backticks under color, and
   a furrow is the same strokes with color and without.
4. **The layer is chosen from what the process read once.** `--color` decides
   first; then `YUNTA_LOOK=plain` turns color off; then `NO_COLOR`,
   `CLICOLOR_FORCE`, `CLICOLOR=0` and whether the stream is a terminal that is
   not `dumb`, as D215 decided. A stream with color draws in the layer
   `YUNTA_LOOK=16` or `YUNTA_LOOK=24` names, else in 24 bits when `COLORTERM`
   is `truecolor` or `24bit`, else in the sixteen. The ground is read from
   `COLORFGBG` and is dark when it says nothing. The terminal is never asked.
5. **Help takes its stream's paint by the same rule**, and so does a usage
   error, on stderr.

## Rationale

A tint, a ground or a pill sets apart what the words already set apart, so a
page grows richer without saying more, and D215's promise holds for every
reader without color: a pipe, a screen reader, a person who turned it off.
Holding the text equal across layers makes that promise a test rather than a
habit: the plain golden is the rich one with its escapes removed. The backticks
cost nothing on a painted line and survive a copy, which a tone does not.
Reading the ground from the environment costs nothing at start, and a wrong
reading is a palette tuned for the other ground, which stays legible.

## Rejected alternatives

**Asking the terminal for its background and its depth.** A query needs a
terminal that answers and a wait for an answer that may never come, at every
start, and tells nothing about a pipe.

**Padding a fill with spaces to reach the edge.** Spaces change the text a
reader copies and the width a cut measures; an erase draws the same band and
writes nothing.

**Letting a surface choose its words by the layer.** Each such choice is a
place where a reader without color reads something else, and the test that
holds the layers equal would need an exception for each.
