---
number: D219
title: "A diagnostic says where in its text it is, beside the entry it names"
status: accepted
revises: [D137]
revised_by: []
---

# D219 — A diagnostic says where in its text it is, beside the entry it names

## Context

D137 took the position out of a diagnostic: the two-pass read worked over a
parsed value that kept no positions, so the field always travelled empty, and
the subject — `task \`t1\`, criterion 1` — located a problem for whoever rewrote
the file. Since D218 the parser that reads every document also locates any value
in its text, so a position is no longer a field nothing fills.

A subject still says *which* entry is at fault. It does not say where a person
with an editor open finds it, and a workflow of forty nodes is read by line.

## Decision

1. **A diagnostic carries `at`** — `{line, col, len}`, counted from one, `len` in
   characters — whenever whoever read the document read it from text: where the
   parser stopped, for a document that did not parse; where the entry the
   subject names is written, for a rule it broke once parsed.
2. **The subject stays**, unchanged: the entry in the document's vocabulary is
   still what a diagnostic is about, and the place is what a reader opens.
3. **A document handed over as a structured value has no text**, and its
   diagnostics carry no `at`. The field is absent rather than empty.
4. **The field is additive** in the events and in `status --json`: a reader that
   predates it never meets it.
5. **A surface that has the text quotes the line**, with carets under the part
   at fault — the value when the value is what is wrong (`implementr` in
   `runner: implementr`), the key otherwise — and a `check` refusal about the
   workflow file is placed the same way, through where the refusal says it is.
6. **A name within two edits of one the document or the config declares is
   suggested**, at the end of the sentence that refuses it, when it is closer than
   every other and the edits are not most of what was typed. A tie suggests
   nothing: a guess offered as an answer costs a reader more than no answer.

## Rationale

A position is only as good as the read that produced it, and here it comes from
the same parse that read the document. The subject keeps what made D137 right:
the rewriter is told which entry, in the document's own words.

## Rejected alternatives

**A position instead of the subject.** A line number moves with every edit above
it; `task \`t1\`` does not.
