---
number: D274
title: "One lexer reads code for every reader of it, and a shape's items are read in Rust, Go, JavaScript or TypeScript, and Python"
status: accepted
revises: [D271]
revised_by: []
---

# D274 — One lexer reads code for every reader of it, and a shape's items are read in Rust, Go, JavaScript or TypeScript, and Python

## Context

Three readers of code each had a lexer of their own, with rules that
differed. Surco's word diff read Rust, Go and JavaScript or TypeScript; the
engine's search for a shape's name read eleven languages and the holes in
their literals; the smells' count of code read Rust. Four tables named a
file's language by its extension, and did not agree. D271 read a shape's
items in Rust alone: a shape in another language read `there` with no
`code`, and its card said `exists, not compared`. The fold of a test file
found each function by rules of its own, line by line.

## Decision

1. **One lexer reads code for every reader of it**: the word diff, the
   colors of code, the items a text declares, a terminal's page of Markdown,
   the engine's search for a name and the smells' measure of code. One table
   of extensions names sixteen languages and the fence name of each, and a
   Markdown fence and a code span are what it reads them to be wherever code
   is drawn. Every byte of a source is in
   exactly one token: a name, a number, a literal's text, a comment, a run of
   space or a sign, and the code in a literal's hole is read as code between
   two pieces of its text. A literal opened by one quote ends at its line's
   break unless its language lets it run on. Masked code turns each comment
   to spaces, and each literal's text too in a programming language; in
   shell, YAML, TOML, JSON and Markdown a quoted word is the same word
   unquoted. This revises D271's consequence that the engine and Surco each
   read Rust with a lexer of their own.
2. **A shape's items are read in Rust, Go, JavaScript or TypeScript, and
   Python**, by one item reader in Surco. Rust's are read by their `;` and
   their braces, as before. Go's, a script's and Python's are read by their
   lines: a statement runs on while a bracket it opened is open, and over the
   lines indented further than its first, as gofmt, prettier and black lay
   code out, so a statement with no `;` never swallows the next. A Go method
   is named by its receiver's type and its name, `Store.Load`, and is looked
   up at the file's top like a function. A script's class and interface, and
   a Python class, hold their methods as an `impl` does, and a Python method
   whose first parameter is `self` or `cls` takes a receiver. Decorators are
   read as attributes are. This revises D271's second point, which read Rust
   items alone.
3. **A shape is read as it writes its ends.** A function the shape writes
   with no body reads today's signature, followed by what ends the shape's
   own: its `;` where it writes one, nothing where it writes none. A block of
   members reads today's header, the members the shape names, and the
   shape's own close: a `}`, or nothing for a Python class, which no brace
   closes.
4. **The fold of a test file reads its functions with the item reader.**
   Where a function starts and ends is the reader's; which function is a
   test stays the fold's: an attribute in Rust, a name in Go and Python, a
   call in a script.

## Rationale

One reading of where a string or a comment ends lets the diff, the colors
and the reading of a shape agree: a lexing error falls the same way on each
of them. Reading by indentation and brackets fits the three languages whose
formatters fix their layout, and needs no grammar. Ending a reading as the
shape ends it keeps the word diff on what changed: a signature the shape
writes without a `;` does not read as one removed.

## Rejected alternatives

- **A library of grammars, such as tree-sitter.** It reads every language
  exactly, and adds a grammar compiled into the binary for each language, in
  a binary kept under a size ceiling, for a card that compares words.
- **One reader per language.** Go, scripts and Python share their layout
  rule; three readers would repeat it.
- **Reading Go's and a script's items by their braces, as Rust's.** A
  statement with no `;`, as Go writes and a script under automatic
  semicolons does, would run to the next `{` and take the next item's body as
  its own.

## Consequences

- A shape in a Go, JavaScript or TypeScript, or Python file carries its
  tree's `code`, and its card shows what changes where it said `exists, not
  compared`. A log written before keeps its readings as they were.
- The lexer reads what a reader of each language sees in one file: YAML's
  block scalars and doubled quotes, shell's here-documents, a script's
  regular expressions where a value starts, Markdown's fences and code spans
  as CommonMark reads them at the top of a document. A fence in a list item
  is read by its indentation alone, three spaces at most, so one nested
  deeper is prose to the lexer while the Markdown surface, which reads list
  items, still closes it. It does not read JSX's text: an apostrophe in it
  opens a literal that ends at its line.
- A layout no formatter writes can mislead the line reader: a statement
  whose continuation is indented no further than its first line ends there.
  A Go group, `const (`, declares one item for each of its lines, read with
  the group's keyword; a statement that declares several names declares its
  first; a script's class field that holds no function is not an item, and
  neither is a method whose name starts with `#`. A decorator stands above
  what it decorates or before it on its line.
- A hole nested more than 64 deep in a literal is read as the literal's text,
  and a Markdown fence nested more than 64 deep in another keeps its code
  plain, so no source can exhaust a reader's stack.
- The payload's `code` doc and the event spec name the four languages.
