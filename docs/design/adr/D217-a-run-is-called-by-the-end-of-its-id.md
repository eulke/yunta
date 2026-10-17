---
number: D217
title: "A run is called by the end of its id, and every command finds it by any unambiguous part"
status: accepted
revises: [D170]
revised_by: []
---

# D217 — A run is called by the end of its id, and every command finds it by any unambiguous part

## Context

Every command that takes a run asked for its whole ULID: 26 characters a person
copied from one line into the next. The ids of a day's runs share their first ten
characters — the time they were made — so the part of an id that told two runs
apart sat at its end, and a person read past a column of identical heads to find
it. Nothing let a person say "the run I just made" or "the one waiting on me".

## Decision

1. **A run's handle is the last six characters of its id.** For a ULID that is its
   random part: two runs made in the same millisecond still differ there.
2. **Every line a person reads names a run by its handle**: the closing block,
   the run list, the live region, the advice that names the next command.
3. **The whole id stays where a program reads or a path is named**: `--json`
   (which carries both, `run_id` and `handle`), `--quiet`, the receipt, the
   `status` headline, a line that names a run's directory, and every message the
   control plane can return.
4. **Every command that takes a run accepts** the whole id, any part that starts
   or ends it in either case, `last` — the newest run of the repository it runs
   in — and `needs` — that repository's run waiting on a person, grouped as the
   run list groups it.
5. **Something two runs answer to is refused**, listing every whole id it could
   mean. Nothing is guessed.
6. **The control plane takes the whole id only.** A program has no reason to
   type less, and a handle that is unique today need not be tomorrow.

The handle's length is registered in D170.

## Rationale

Six characters of Crockford base32 are thirty bits: two runs on one machine
sharing a handle is a collision nobody meets, and when one does, the refusal
lists both. A part that starts the id is what a person who copied the head of
one types; a part that ends it is what one who read the handle types.

## Rejected alternatives

**A counter per repository (`#12`).** It needs a store that assigns it, and two
checkouts running at once assign the same number.

**A prefix only.** The head of a ULID is the time, which every run of a day
shares: a prefix that tells two runs apart is most of the id.
