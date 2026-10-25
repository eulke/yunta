---
number: D271
title: "A plan's shapes are read in its tree once, when it is registered, and every page draws that reading"
status: accepted
revises: [D239, D270]
revised_by: []
---

# D271 — A plan's shapes are read in its tree once, when it is registered, and every page draws that reading

## Context

A card carries the code each task writes, each shape and each change once
(D270). Drawn from the plan alone, it cannot say what that code is to the tree
the plan was written against. A shape the tree does not hold reads the same as
one the plan widens; a widened signature says nothing of the files that name
it; and which of its words change shows only to a person who opens its file
beside the card.

The engine already read that tree when a plan was registered, and kept the
reach alone. D239 looked each shape up to derive what its owner may write: one
`git grep` per shape, among the files of the shape's own extension, then one
`git show` per file it found beyond the owner's scope, to keep the files whose
code names the shape. What a shape's own file holds of it was never read. The
search ran from the directory the run's tree sits in and named files from
there, while `show` read them from the top of the repository, so a run whose
tree sits below its repository's top could fail at registration. A `.tsx` file
calling a `.ts` shape was never searched.

## Decision

1. **One reading of the tree, when a plan is registered, gives two facts.** When
   a tasks document is registered, at birth or when a node produces it, the
   engine reads the tree once, at its HEAD. `shapes_read` names that commit,
   and so does every `scope_derived` the same registration writes. One
   `git grep` searches for every shape whose name is one word of code, among
   every file of each shape's language family, whatever git's attributes say of
   it — a `.ts` shape reaches `.tsx` files, and an extension the engine does
   not read is a family of its own — from the top of the repository, naming
   each file as the repository does wherever in it the run's tree stands. It
   never enters a submodule, whatever `submodule.recurse` says. One batched
   read takes the text of the files it found, and D239's filter keeps, in
   memory, the ones whose code names the shape; a file with a NUL byte in its
   first 8000 bytes is not code. The reading gives each task's reach, written
   as `scope_derived` when it changes, as before, and what the tree held of the
   plan's shapes, written as `shapes_read` for the plan by the hash of the
   bytes the run accepted. A reading is written again only when it differs from
   the one the log holds for that plan, compared in the form the log stores it:
   redacted. This revises D239's first point.
2. **What `shapes_read` holds.** The plan, the commit, and each shape the
   reading follows, in the plan's order: one whose name is one word of code, in
   a file whose extension says its language. Each comes with its owner, its
   file as the repository names it, and `in_tree`. `in_tree` is `new` where its
   file's code does not name it, the file is not code, or the file does not
   exist: the plan makes it. It is `there` where its file's code names it, with
   `code` and `outside`. `code` is what the file holds of the items the shape
   declares, read the way the shape writes them: a function to its signature
   where the shape stops at one, an `impl` or a `trait` to the members the
   shape names, attributes only where the shape writes some. The engine reads
   Rust items; a shape in another language, or one whose file only uses its
   name, is `there` without `code`. `outside` is `files` with `paths`, each
   file whose code names the shape other than its own and every file a task of
   the plan changes, measured by the changes the plan declares and not by
   scopes; or `common` with the number of `files` of its language family that
   carry the name when more than 20 do. Of those, only the shape's own file is
   read, for its code and to see that its code names the shape; none is read to
   follow the name.
3. **Every page draws the reading the log holds.** Between what a task keeps and
   what tests it, a card says what each shape implies: for a shape its file
   holds, the files beyond the plan whose code names it and no task changes, a
   line each, or how many files carry the name when they are too many to
   follow. A shape whose code reads `unchanged` implies nothing, and one whose
   code the engine does not compare, `exists, not compared`, always implies its
   files. The task that builds the shape may write every file it lists, within
   its scope or its reach. Under every code block a line says its size and,
   before it, for a shape the engine read, what the code is to the tree: `new`;
   `modified`, with the words it puts in marked `{+…+}` and the words it takes
   out `[-…-]`; `unchanged`; `exists, not compared`; or
   `modified, too long to mark`. A gate's prompt, `resolve-gate`,
   `status --node` and a plan a forge publishes draw that one record, and none
   of them runs git. This revises D270's second point.
4. **The diff is drawn, not recorded.** The log keeps the code the tree held, as
   a value. The word diff between it and the plan's code is a function of the
   two, computed each time a page is drawn, the same on every surface.
5. **`status --json` carries the readings as data.** The run document carries
   `shapes_read`, one per plan the run holds, in the order the run first
   accepted them, and leaves it out when the run read none. It is an addition,
   so the document stays at version 8.

## Rationale

Which files name a shape is one question, and it gets one answer: one search at
one commit, through one filter that reads code, gives both the reach and what a
card implies. HEAD at registration is the tree the planner read, and the commit
the reach was already read at; it differs from the run's base commit exactly
when work landed before the plan was made again. Recording what it held lets
every page say what the tree held when the plan was written, wherever and
whenever it is drawn, including where no repository can be reached, and keeps
`status --json` derived from the log alone. Drawing a plan runs no git.

One search and one batched read cost less than a search per shape and a read
per file beyond the owner's scope. Measured on real runs of the workflow that
writes a spec and a plan, with the mock adapter and five runs each: a plan of
five tasks and seven shapes went from 14 git processes at its registration to 3
(one `rev-parse`, one `grep`, one `cat-file`), its median registration from
131.9 ms to 39.1 ms, and the median close of the node that produced it from
328 ms to 237 ms. The old lookup read no file when no file beyond the owner's
scope named the shape, so a plan of one shape that the tree names only inside
its owner's scope pays one git process more than before: the batch that reads
the shape's own file, about 7 ms. That read is what tells a new shape from one
the tree holds, and it gives a card the code it compares.

## Rejected alternatives

**Reading the tree when a page is drawn.** Every draw, every `status --json` and
every `workflow_status` poll of a run parked on a plan would run git; a page
drawn where no repository holds the commit would say less; and D239's question
would get a second answer, at another commit.

**Reading at the run's base commit.** Once work lands and the plan is made again,
the base commit is not the tree the planner read.

**Extending `scope_derived`.** It is per task, it is the fence's fact, and it is
written only when a task's reach changes. What the tree held of a shape is per
plan, and a reader needs it for shapes that reach nothing.

**Recording the diff.** The log keeps values, not pictures of them (D268). A
recorded diff repeats two values the log holds once, and a better way of drawing
it never reaches the runs that recorded the old one.

**A diff drawn under the code.** The shape would be drawn twice; its words marked
inside its own lines say the same once.

**Following the items a change names.** D239 gives them no reach, and names as
common as `add` and `list` would fill a card with rows that imply nothing. An
item a change touches is followed once the plan declares it as a shape.

## Consequences

- A plan whose shapes are words of code costs its registration three git
  processes however many shapes it declares: one for its commit, one search,
  and one batched read, which a search that finds no file skips; a file whose
  path holds a line break, which a batch cannot name, is read with a process of
  its own. The lookup it replaces cost one for the commit, one search per shape
  and one read per file beyond the owner's scope, so a plan that cost more than
  three pays less, and a plan of one shape pays one more when the tree names it
  only inside its owner's scope, or when its own file carries a name too common
  to follow.
- A tree below its repository's top reads the whole repository and names its
  files from the top, as the fence does. A plan written with paths relative to
  that tree reads its shapes as new.
- A shape's reach follows its language family: a `.tsx` caller of a `.ts`
  shape, or a `.c` caller of a shape in a header, joins its owner's reach, so
  an owner may write more files and more batches are kept apart by D239's fifth
  point. The 20 files past which a name is too common to follow are counted
  across the family too, so a name its whole family carries in more than 20
  files reaches nothing, even where the files of its own extension carry it 20
  times or fewer: a `.ts` shape named in 15 `.ts` files and 10 `.tsx` files
  reached its `.ts` callers beyond its owner's scope before, and now reaches
  none. For a Rust tree at the top of its repository the reach is the same as
  before, save a `.rs` file that is not code, which no longer reaches.
- The search reads every file of the family whatever git's attributes say of
  it, so a file marked `binary` or `-diff` is searched like any other, and what
  is not code is told by its bytes: a file with a NUL byte in its first 8000
  bytes neither reaches nor is outside, and a shape whose own file is one reads
  new. The search never enters a submodule, so a person's `submodule.recurse`
  changes nothing it finds.
- A file that names a shape need not call it: a re-export or an import names it
  too, which is why a card says "named". An alias made with `use … as`, a caller
  in another repository and a shape moved from another file are not found; a
  moved shape reads as new.
- The item reader can pick the wrong item where a macro makes items, where an
  inline module other than a test module holds a namesake, or where `cfg` twins
  declare one and the plan writes neither twin's attributes: it then reads the
  twins in turn, the first for the first planned. A planned function that takes
  `self` is read among methods only, and an `impl` member under the header of
  the block that holds it. Where the reader cannot tell, a shape reads `exists,
  not compared` or `new`.
- A shape the plan abbreviates shows the words of the tree's code it leaves out
  as removed, which is what the plan's text says.
- The marks can collide with code such as `vec![-1]`. Color tells them apart;
  plain output can, rarely, be ambiguous, and inside a Markdown fence the marks
  disturb its highlighting.
- The engine and Surco each read Rust strings and comments with a lexer of their
  own. The diff reads both texts with Surco's, so a lexing error falls the same
  way on both sides.
- A secret in the tree's code is stored redacted, like the rest of the log, and
  the diff shows its placeholder as removed. A new reading is compared with the
  stored one in that redacted form, so a secret alone never writes a reading
  again.
- The search's output grows with the occurrences it finds: a name common in a
  large repository can come to megabytes, read once per registration.
- `shapes_read` is the 63rd kind, and additive: a binary that predates it keeps
  the event and counts it among the unknown kinds. A run writes one when it
  registers a plan whose shapes are words of code, and again only when a later
  registration of that plan reads something else; most of it is the tree's
  code of the compared shapes, and it moves the `seq` of every event after it.
- A log written before `shapes_read`, and a tasks document registered without an
  acceptance under its producer, draw their cards with sizes only.
- The payload spec, the contract's event table, the task spec, the guide and the
  compatibility policy state the new kind, the card's rows and the run
  document's field, and `docs/design/adrs.md` is generated from the decision
  files.
