#!/usr/bin/env python3
"""Report Rust functions whose bodies exceed a line budget.

A function that no longer fits on a screen is a signal the repo attends to
in the PR that crosses it (CLAUDE.md). This ratchet makes that signal
mechanical: it walks the given files, measures every `fn` body by brace
depth, and exits non-zero naming each function over `--max` lines so CI and
a contributor see the same list.

Body length is counted from the line after the opening `{` of the function
body to its matching `}`, inclusive of blank and comment lines — the whole
span a reader scrolls. Attributes, the signature, and the closing brace line
are not counted. String and char literals and line/block comments are
skipped so a brace inside them never opens or closes a body.
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Finding:
    file: str
    line: int
    name: str
    length: int


def _strip_noise(line: str, in_block_comment: bool) -> tuple[str, bool]:
    """Return the line with string/char literals and comments blanked, plus
    whether a block comment is still open at end of line. Only braces need to
    survive, so blanked spans become spaces."""
    out = []
    i = 0
    n = len(line)
    while i < n:
        c = line[i]
        if in_block_comment:
            if c == "*" and i + 1 < n and line[i + 1] == "/":
                in_block_comment = False
                out.append("  ")
                i += 2
                continue
            out.append(" ")
            i += 1
            continue
        if c == "/" and i + 1 < n and line[i + 1] == "/":
            break  # line comment — rest is noise
        if c == "/" and i + 1 < n and line[i + 1] == "*":
            in_block_comment = True
            out.append("  ")
            i += 2
            continue
        if c == '"':
            out.append(" ")
            i += 1
            while i < n:
                if line[i] == "\\" and i + 1 < n:
                    out.append("  ")
                    i += 2
                    continue
                if line[i] == '"':
                    out.append(" ")
                    i += 1
                    break
                out.append(" ")
                i += 1
            continue
        if c == "'":
            # A char literal or a lifetime — both are noise for brace
            # counting; blank to the next `'` when it is a literal, else
            # just this char (a lifetime has no closing quote).
            j = i + 1
            if j < n and line[j] == "\\":
                j += 2
            else:
                j += 1
            if j < n and line[j] == "'":
                for _ in range(j - i + 1):
                    out.append(" ")
                i = j + 1
                continue
            out.append(" ")
            i += 1
            continue
        out.append(c)
        i += 1
    return "".join(out), in_block_comment


def scan(path: Path) -> list[Finding]:
    findings: list[Finding] = []
    lines = path.read_text().splitlines()
    in_block_comment = False
    # Stack of open functions: (name, line, depth_at_body_open).
    open_fns: list[tuple[str, int, int]] = []
    depth = 0
    pending_fn: str | None = None  # a `fn name` seen, body brace not yet open
    for idx, raw in enumerate(lines, start=1):
        clean, in_block_comment = _strip_noise(raw, in_block_comment)
        # Detect a function signature start on this line (before counting
        # braces), so `fn f() {` opens a body at the right depth.
        stripped = clean.strip()
        if pending_fn is None and (" fn " in f" {stripped} " or stripped.startswith("fn ")):
            # Only a real definition, never a `fn` inside a type like
            # `dyn Fn(...)` (capital F) — the token is lowercase `fn`.
            after = _fn_name(stripped)
            if after is not None:
                pending_fn = after
        for c in clean:
            if c == "{":
                if pending_fn is not None:
                    open_fns.append((pending_fn, idx, depth))
                    pending_fn = None
                depth += 1
            elif c == "}":
                depth -= 1
                if open_fns and open_fns[-1][2] == depth:
                    name, start, _ = open_fns.pop()
                    length = idx - start - 1
                    findings.append(Finding(str(path), start, name, length))
    return findings


def _fn_name(stripped: str) -> str | None:
    # Grab the identifier after the `fn` keyword.
    tokens = stripped.replace("(", " (").split()
    for k, tok in enumerate(tokens):
        if tok == "fn" and k + 1 < len(tokens):
            name = tokens[k + 1]
            for stop in ("(", "<"):
                name = name.split(stop, 1)[0]
            return name or None
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Flag Rust functions over a line budget.")
    parser.add_argument("--max", type=int, required=True, help="maximum body lines")
    parser.add_argument("paths", nargs="+", help="Rust files or directories to scan")
    args = parser.parse_args()

    files: list[Path] = []
    for p in args.paths:
        path = Path(p)
        if path.is_dir():
            files.extend(sorted(path.rglob("*.rs")))
        else:
            files.append(path)

    over: list[Finding] = []
    for f in files:
        for finding in scan(f):
            if finding.length > args.max:
                over.append(finding)

    for finding in sorted(over, key=lambda x: (-x.length, x.file, x.line)):
        print(f"{finding.file}:{finding.line}: fn {finding.name} is {finding.length} lines (max {args.max})")

    if over:
        print(f"\n{len(over)} function(s) over {args.max} lines", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
