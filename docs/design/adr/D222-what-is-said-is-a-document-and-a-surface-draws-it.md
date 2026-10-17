---
number: D222
title: "What a surface says is a document of blocks, and a medium draws it"
status: accepted
revises: []
revised_by: []
---

# D222 — What a surface says is a document of blocks, and a medium draws it

## Context

Each block — a headline, a set of fields, the evidence of a failure, a
decision, the next commands — laid itself out for a terminal: columns padded to
a label width, lines cut at the stream's width, a glyph chosen from the set the
locale allows, paint from the stream's ink. That was the only medium there was.

The engine is about to write the same blocks into files and into pull requests,
which are read as Markdown. Yunta may later be read in a browser or a desktop
application as well. Each new medium would otherwise lay out every surface
again, and the same fact would start being said differently in each.

## Decision

1. **A surface builds a `Doc`** of blocks, in the order a reader reads them. A
   block holds what is said and the role each part plays — the subject, a mark,
   a path, a command — and never a width, an escape or a glyph.
2. **A `Surface` draws a `Doc`** for one medium. `Terminal` lays the blocks out
   as they always were, for a stream's look. Markdown is the next surface; a
   page in a browser or an application would be another implementation of the
   same trait, and no surface that builds a document changes for it.
3. **Every surface draws every block, and every word a block says.** The word
   carries the meaning; glyphs, color and layout only repeat it, so a medium may
   draw them its own way or not at all, but may not drop a word.
4. **A surface that composes lines of its own** puts them in the document as
   lines of spans with roles, which every medium can still read.

## Rationale

The blocks already held their content as data, with the terminal layout in one
method each. Naming that layout as one surface among possible others costs one
dispatch and keeps the content where it was. A trait over a document, rather than
a flag per block, keeps the medium's rules together: the terminal's widths in
one place, Markdown's emphasis in another.

## Rejected alternatives

**A Markdown ink that paints terminal lines.** Lines already carry a terminal's
padding and cuts; a file written from them reads as a terminal pasted into a
page.

**A renderer per medium for each surface.** Every surface would be written once
per medium, and the third medium would cost as much as the first.
