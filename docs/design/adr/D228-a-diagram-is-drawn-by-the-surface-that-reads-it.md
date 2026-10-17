---
number: D228
title: "A diagram is drawn by the surface that reads it"
status: accepted
revises: []
revised_by: []
---

# D228 — A diagram is drawn by the surface that reads it

## Context

A planner draws the change it plans as a Mermaid flowchart, in the plan's
description or its design. A forge draws Mermaid, and a terminal does not. The
review form of a plan said `diagram: in the whole plan`, and the whole plan
printed `(a diagram, drawn wherever Markdown is read)` in its place. A person
deciding at a terminal never saw the picture that says the most about the change
in the least space.

## Decision

1. **A diagram is a block.** It carries its Mermaid source as written and the
   flowchart it reads as. Markdown an author wrote is read into its text and its
   diagrams before a terminal draws it, wherever that Markdown is shown.
2. **A file keeps the source.** A forge page or a Markdown file writes the
   fence as written, so the forge draws it and the file stays byte for byte.
3. **A terminal draws what it reads.**
   - It reads a flowchart: its direction, boxes with their shapes, and links
     with their strokes and labels.
   - It draws the chart in boxes reading the way the author wrote it when that
     fits the line. When only the turned chart fits, it draws that. When neither
     fits, it writes one line per chain.
   - No line runs past the terminal's width, and no label is dropped. A chart
     that reads up or left is drawn reading down or right: the same boxes and
     links, the other way up.
4. **What it does not read is shown as written.** A subgraph or another kind
   of diagram is printed as its source, with a note that it is drawn where
   Markdown is read. It is never guessed at.
5. **The layout is fixed:**
   - boxes in layers by their longest path;
   - each layer ordered by the average place of what it links with;
   - every link on a lane of its own, a link that spans layers passing through
     each one between, and a link that closes a cycle going back along a lane
     at the side;
   - each label beside the head it ends in.

   A box's label wraps at 24 cells, and a link with a longer label is written
   as a chain. Two boxes side by side sit 4 cells apart. The glyph set draws
   every joint, corner and head, so an ASCII terminal reads the same chart.

## Rationale

The diagram is the planner's fastest account of the change. The person who
reads a plan at a terminal is the one who approves it, so the picture belongs
where the decision is made. Each surface already draws the blocks it is given
its own way, and a diagram is one more block. A forge already draws Mermaid
well, so the source goes to it untouched. Falling back from boxes to turned
boxes to chains keeps the guarantee every other block keeps: nothing runs past
the line, and nothing the author wrote is lost.

## Rejected alternatives

**Rendering with the Mermaid toolchain.** It needs a JavaScript runtime and
draws an image, which a terminal cannot show.

**Asking the planner for ASCII art beside the Mermaid.** That makes two drawings
of one change that drift apart, and the terminal's width is not known when the
plan is written.

**Only the chains.** One line per chain loses what the boxes show at a glance:
where a question branches, where links meet, and where a cycle goes back.
