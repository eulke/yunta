---
number: D211
title: "NO_COLOR takes the color away and leaves the region"
status: accepted
revises: [D162]
revised_by: []
---

# D211 — NO_COLOR takes the color away and leaves the region

## Context

D162 listed `NO_COLOR` among the reasons the live view stands down to one line
per event, beside a stream that is not a terminal and `TERM=dumb`. Its reading
was that a reader who asked for no color asked for output that reads the same
however it is captured.

That is not what the convention asks. `NO_COLOR` asks a program not to add ANSI
color to its output; it says nothing about how the output is laid out. A person
who sets it once in their shell, because colors are hard to read on their
terminal or for their eyes, lost the region on every run: the one place that
says what is moving and whether anything needs them.

## Decision

1. **`NO_COLOR` leaves the delivery alone.** The region is drawn on a terminal
   whatever `NO_COLOR` says; only a stream that is not a terminal and
   `TERM=dumb` stand it down.
2. **It takes the color away**, wherever a surface paints one.

## Rationale

The region is plain text. What `NO_COLOR` can remove from it is color, and
removing the region to remove the color removes what the reader came for.

## Rejected alternatives

**Keeping the downgrade and documenting it.** It turns a request about color
into a request about layout that nobody made.
