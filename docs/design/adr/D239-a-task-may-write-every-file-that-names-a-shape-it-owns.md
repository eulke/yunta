---
number: D239
title: "A task may write every file that names a shape it owns"
status: accepted
revises: [D73]
revised_by: []
---

# D239 — A task may write every file that names a shape it owns

## Context

A plan declares each shape it changes once, in the file it lives in, owned by the
one task that builds it. Changing a shape reaches past that file: a widened
signature breaks its callers, a grown enum breaks its matches. The owner's scope
covered the shape's file and rarely its callers, because a planner does not list
every place a name is used. So the owner's build failed on a caller it was not
allowed to touch; the session had to ask for scope and invent a criterion to
justify it, and the rule refused the criteria that only restated the request.
The red output named the exact place, and the plan named the shape and its
owner: everything needed to allow the write was already known.

## Decision

1. **The engine derives what an owner may write.** When a tasks document is
   registered, at birth or when a node produces it, each shape whose name is an
   identifier is looked up in the run's tree as a whole word, in files of its
   own file's type (`git grep -lwF` at the tree's commit). Every file found
   outside its owner's declared scope whose code names it joins that owner's
   reach: an occurrence inside a string literal or a comment — a test
   fixture quoting a plan, a doc mentioning the shape — calls nothing, and
   would only keep the owner from sharing a batch with the task whose file
   it is. A language the engine cannot read is read whole.
2. **A name too common reaches nothing.** A shape named in more than 20 files
   derives no file; the event lists it under `common`, so a reader sees why.
3. **Its own event.** `scope_derived` states, per task, the files, the shapes
   they name, the common names and the commit they were read at. A later
   derivation replaces the earlier one; one equal to what the log states is not
   written again. It is no grant: it does not count toward `max_per_run` and
   leaves the task's registration — and so a recut — alone.
4. **One reach for every reader.** A task's fence, its tools, the audit of its
   close and the re-verification after a rebase all read its declared scope
   plus its derived reach plus what was granted to it.
5. **Reaches that meet never share a batch.** Declared scopes of independent
   tasks are disjoint by rule; derived reach may cross into another task's
   scope. The batch selector keeps two tasks whose reaches might meet apart:
   the later one runs in a following batch, on the tree the first landed in.

## Rationale

The engine verifies facts rather than asking for them: which files name a shape
is a fact of the tree, as checkable as a criterion. Deriving it removes a stop
that no person had anything to decide about, and the work it unblocks is
exactly the work the plan already assigned to the owner.

## Rejected alternatives

**Requiring the planner to list every caller.** It is what failed: a planner
does not read every use of a name, and the omission only shows when the build
breaks.

**Requiring every shape name to be an identifier.** Shapes also declare file
formats and schemas, which no identifier names. Those derive nothing, and their
owners ask for scope as before.

**Forbidding derived reach to cross another task's scope.** It would drop the
caller the owner needs precisely when another task works near it. Serializing
the two tasks costs one batch and keeps both correct.
