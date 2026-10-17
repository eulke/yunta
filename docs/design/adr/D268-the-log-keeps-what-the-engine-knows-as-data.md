---
number: D268
title: "The log keeps what the engine knows as data; an error keeps its own report"
status: accepted
revises: [D163]
revised_by: []
---

# D268 — The log keeps what the engine knows as data; an error keeps its own report

## Context

D133 made `node_failed` carry its failure as data and left the prose to the edge
that shows it. D163 made an escalation's evidence a list of labelled facts. Both
still left sentences in the log, composed by the engine: an option's label and
trade-off ("On to `ship`", "resolves this gate; `ship` runs next"), the gate's
assignee as a fact, the budget and loop caps as facts, a scope request's paths in
its evidence, the title and detail of a finding the engine reported, the size and
digest of a note written as "135 bytes, sha256 2eb6d565235b", the clause that a
refused document's rule broke, the sentence for a denied command, and the sentence
for a failed engine step. Real runs still wrote them after D133 and D163.

A sentence in the log cannot be read again for another reader, cannot be counted,
and cannot be corrected when the wording improves. An agent or a program that reads
the log gets prose where it needs a value. The engine also wrapped a failing call's
own report in a sentence of its own, so the report and the engine's words about it
were stored as one string.

## Decision

1. **The log holds values, and the engine composes no sentence for it.** Three kinds
   of string stay in the log. What the run received: commands and their output, the
   words of an agent, a CLI, a person or a reviewer, and inputs. Identifiers: ids,
   paths, the branch name, step and tool names, a source's id, the reference and URL
   of a pull request. And the report of an error: the words the failing call raised,
   with its causes, whoever wrote them. An error keeps its own report, and the engine
   adds nothing around it. What the engine used to add, such as which step failed,
   is data.

2. **A menu is data, one per escalation.** An escalation carries one `Menu` that
   says what it is about, with the data its words are written from: the gate's
   assignee, the budget and loop caps, the failure or the departure, the paths of a
   scope request, the console or the published pull request. An option carries its
   id and, when an author declared it, where it leads (`Leads`). The question an
   option asks follows from that (`GateOption::asks`). Claims are typed values, and
   a withheld option carries the flaws of the plan it would take the run past. Render
   writes each label, trade-off, question and fact from this data.

3. **An engine finding carries its cause.** A finding a session wrote keeps its title
   and detail. A finding the engine reported keeps what it found (`FindingCause`), and
   render writes its title and detail. Two findings are one complaint when their
   normalized titles match today, and the complaint key is built from the parts of
   the cause that the title names. Agents and programs read the cause as data. A
   findings document the run holds carries entries with a cause, and a document a
   session hands over keeps the strict shape it had.

4. **A rule's clause is data.** A diagnostic's rule carries its `Fault`, one value per
   rule family. The clause is written beside the rule when the diagnostic is read,
   by `Fault::clause` in core, and nothing stores it.

5. **Older logs keep their words.** A field that held words becomes data plus a
   `LoggedWords`, which reads the sentence an older log stored, shows it as written,
   and writes it back as it was read. Production code obtains one only by reading a
   log. Each changed payload is a superset of the earlier one, so every line an
   earlier binary wrote still reads. No kind is renamed or added, and `KIND_VERSION`
   stays 1, as D141 allows before the first published tag.

6. **A note keeps its size and the head of its digest.** A session's note is written
   as its byte count and the first twelve hex digits of its sha256, and never as a
   sentence. An executor that exits without a summary is an engine fact, and render
   writes its sentence as it always did.

## Rejected alternatives

**Typing every error of every crate.** It would be a much larger change, and the
decision a person makes does not depend on the type of an error. The report of a
failing call stays its own text.

**Keeping `title` and `detail` beside a finding's cause.** Two versions of one fact
drift apart, and the stored words would be the ones a later wording replaces.

**Moving the rule clauses out of core into each surface.** The clauses are about
eighty sentences, and every surface would repeat them.

**Letting an agent write its own words for an engine finding.** Those words would
duplicate render's, and an agent reads data well.

**Keeping the engine's facts beside the menu's.** The same fact would be written
twice, and the engine's copy would be the one nobody checks.

## Consequences

- A line written by this binary carries no sentence the engine composed. A log an
  earlier binary wrote reads, shows and exports with the sentences it stored.
- Render is the one writer of the words a person reads for an escalation, a finding,
  a failure or a rule. The console, `yunta status`, the chronicle, the closing block
  and `status --json` read the same data.
- `status --json` stays at version 7, which no release has published. Its
  `decision.evidence` lists the facts a person reads, and `decision.failure` holds the
  failure as the log keeps it.
- `resolve-gate` without a terminal adds `--text` to the command of each option that
  asks for words, so a command copied from it is not refused.
- A gate that shows an engine finding writes that finding's cause into its view, so
  the view's file and its hash change. A resume is still judged against the menu the
  escalation offered, not against what was shown.
- `Escalation::new` takes its menu and returns no error. A claim cannot repeat a
  record that holds no words, so the refusal and its error type go.
- Tests that asserted words in the log assert the data, and the words are asserted
  where render writes them.
- `docs/design/adrs.md` is generated from the decision files.
