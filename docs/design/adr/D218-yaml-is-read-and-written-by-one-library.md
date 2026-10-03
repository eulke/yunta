---
number: D218
title: "YAML is read, written and located by one library"
status: accepted
revises: [D112]
revised_by: []
---

# D218 — YAML is read, written and located by one library

## Context

`serde_norway` reads every document and says where it stopped only when it
could not read one. A workflow that parses and then breaks a rule — two nodes of
one id, a `depends_on` naming nothing, a runner no layer declares — has no
position to show, and those are most of what `yunta check` reports. The libyaml
under `serde_norway` keeps a position for every event, but reaches Rust only as
`unsafe` functions over raw pointers, and the workspace forbids `unsafe`.

A second parser kept beside the first to find positions would read every
document twice with two grammars, and the day the two disagree about a document
a diagnostic points at a place the first parser never read.

## Decision

1. **`serde-saphyr` reads and writes every YAML document**, behind
   `yunta_core::yaml`, the one door every document goes through. Its parser,
   `granit-parser` (a fork of `saphyr-parser`), is the same parser that locates a
   value in a text once a document has a problem: one grammar for reading a
   document and for saying where something in it is.
2. **A document is read as YAML 1.2**: only `true` and `false` are booleans, so
   a node named `n` and a gate option `on` are the words they look like. A key
   written twice in one mapping is refused.
3. **Digits grouped with `_` read as the number they spell**: `2_000_000` is a
   limit of two million, never a string.
4. **A refusal is worded as serde words it for every format**: the names a
   document may use in backticks, the path once — before the sentence, never
   repeated inside it — and the place, at the key where it is written.
5. **What the engine writes keeps a line of text on one line**, however long, and
   text with lines of its own in a literal block.
6. **`yunta_core::yaml::Value` is this system's own**, an ordered tree a custom
   deserializer reads a document into before choosing its type, and that a
   persisted document migrates and orders its keys with. A key with nothing after
   it reads as an empty list or mapping where one is expected.

## Rationale

The position of a value is a property of the text it was read from, and one
library that both reads the text and says where things are in it cannot place a
value somewhere it did not read it. `serde-saphyr` is safe Rust under
`MIT OR Apache-2.0`; every run already on disk reads back with it.

## Rejected alternatives

**`serde_norway` with `saphyr-parser` beside it.** Two grammars over one text.

**Locating keys by indentation.** Wrong on flow mappings — `{ id: build, kind:
bash }` — which this repository's own workflows and fixtures write often.

**Positions only for documents that do not parse.** The rules a parsed document
breaks are what a person corrects most, and they would have no place to point to.
