---
number: D216
title: "A line is laid out to its stream's width, between a floor and a ceiling"
status: accepted
revises: [D170]
revised_by: []
---

# D216 — A line is laid out to its stream's width, between a floor and a ceiling

## Context

Every surface was laid out at eighty cells. On a terminal wider than that, a
decision's tradeoff wrapped at eighty and left the rest of the row empty; on one
narrower, a printed line wrapped wherever the terminal ran out of row, and the
live region cut its rows to the terminal's edge. The prompt measured the
terminal for its typing row and its menu, and capped its prose at eighty. The
width a line got depended on which surface printed it.

## Decision

1. **A line is laid out to the width of the stream it is written to.** A
   terminal is measured; `COLUMNS`, when it names a positive number, is the width
   a reader asked for and wins over the measured one, on a terminal or a pipe.
2. **That width is held between a floor of 60 cells and a ceiling of 120.**
   Below the floor a column of names and what sits beside it no longer share a
   row, and the terminal wrapping a line laid out at the floor reads better than
   a surface cut to fit. Above the ceiling a line is too long to read in one
   sweep.
3. **Off a terminal, with no `COLUMNS`, a line is 80 cells** — the width it
   still has to survive once pasted into a review, an issue or a log.
4. **A row redrawn in place never passes the terminal's edge**, floor or not:
   the live region and the prompt redraw their rows, and a row that wraps tears
   the next redraw.

The floor and the ceiling are registered in D170.

## Rationale

The width is a property of where a line lands, not of what it says, so it is
decided once per stream and every surface reads it. `COLUMNS` is the convention
for asking a program for a width, and a shell does not export it unless asked to.

## Rejected alternatives

**The terminal's full width, unbounded.** A tradeoff two hundred cells long is
read by moving the head, not the eyes.

**Eighty everywhere.** It is the width a pasted line survives at, and the
narrowest a reader at a terminal wider than that is ever offered.
