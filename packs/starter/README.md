# yunta/starter

Two minimal workflows that teach Yunta's own shape — nothing more. This pack
is a fixture as much as it is a teaching tool: it installs and removes like
any third-party pack, with no special status the engine grants it (D57).

## `fix`

The smallest verified unit there is: one `prompt` node with a declared
`scope`, checked by a `bash` node before the work counts as done.

```bash
yunta run yunta/fix --input issue="the login button is misaligned"
```

Swap the `verify` node's command for your own test suite once you copy this
into your own repo — the marker-file check here exists so this workflow
runs identically on any machine, with nothing installed beyond Yunta itself.

## `review`

The same prompt runs once per `runners:` entry (`reviewer`, `reviewer-alt`),
each producing its own findings file — a fan-out review where neither
runner's read anchors the other's.

```bash
yunta run yunta/review
```

## Installing

```bash
yunta pack add <source-of-this-pack>
```

Both workflows are then addressable as `yunta/fix` and `yunta/review`. Your
own `runners:` needs `executor`, `reviewer` and `reviewer-alt` resolvable —
`yunta doctor` says so if it doesn't.
