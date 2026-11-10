# plan: 4 tasks in 3 steps, 2 risks

## ◆ fragua (standard) · needs you · for 32m10s · 7E5PH4

approve-plan asks you to approve the plan before anything is built.

Then the run builds the change and checks what it built.

## what it found

- ✓ **git** commits as Ada \<ada@example.com>
- ▲ **forge** github acme/web, reachable
- ◌ **review** no fan-out review

> [!CAUTION]
> - A name with a newline in it breaks the line it is printed on.

- ▲ the budget sits under this workflow's usual spend
  - so: the run may stop before it finishes
  - fix: raise `limits.max_tokens_per_run`

## ✗ node `lint`: exit 101

```
line 1
line 2
line 3
```

whole output: `yunta status 7E5PH4 --node lint`

error: unused import  
  --> src/lib.rs:3:5

**error** node `fix`: unknown key `promt` for a `prompt` node  
at `.yunta/workflows/lint-fix.yaml:9:5`

```text
9 │     promt: "Fix what lint reported."
  │     ^^^^^
```

_help:_ did you mean `prompt`? · `yunta schema workflow` lists every key

**warning** the program `cargo-nextest` is not on this machine's PATH

**error:** no run is called `7E5PH5`; did you mean `7E5PH4`?  
_looked in ~/.yunta/runs_

- ✓ **2 healthy** claude-code 2.1.302 · git
- ▲ **first run** a new binary's first run takes 469 ms here, its next 2 ms
  - so: macOS assesses each new executable before its first run, and a suite pays that for every test binary it links again
  - fix: add the terminal that runs yunta under Developer Tools, then open it again
- ✗ **runners** none declared, and `lint-fix` needs one
  - fix: declare a runner in .yunta/config.yaml:

    ```
    runners:
      implementer:
        - { adapter: claude-code, model: <model> }
    ```

outside the declared globs:
- `src/main.rs`
- `Cargo.lock` (the lock file)

<details>

<summary><b>T001</b>: Make it</summary>

the file exists

</details>

A change **in Markdown**, with `code`.

- an item

```rust
fn main() {}
```

```mermaid
graph LR
  plan --> build
```

`src/lib.rs`: the change

```rust
pub fn greet() {}
```

_1 line_

`src/greet.rs › hello`: greets the person by name

```rust
pub fn hello({+name: &str+}) -> String;
```

_modified · 1 line_

`src/greet.rs › bye`: says goodbye

```rust
pub fn bye(name: &str) -> String;
```

_new · 1 line_

| step | task | |
|---|---|---|
| ● 1 | `cli-global` | Global and project flags |

| | node | | time | tokens |
|---|---|---|---|---|
| ✓ finished | `plan` | 4 tasks in 3 steps | 1m31s | 1.94M |
| ✓ finished | `approve-plan` | approved on its third round | 6h27m |  |
| ◌ never ran | ├─ `review` |  |  |  |
| ◌ never ran | └─ `conform` |  |  |  |
| ◌ never ran | `lint` ↺ fix-lint | sends a failure back at most once |  |  |

| | run | workflow | where it stands | |
|---|---|---|---|---|
| ◆ needs you | `EKPBGE` | fragua (standard) | implement asks at its terminal | 32m |
| ■ stopped | `60Y934` | fragua (quick) | stopped while it ran | `yunta resume 60Y934` |

| file | added | removed |
|---|---|---|
| `crates/cli/src/pack.rs` | +142 | −38 |
| `docs/notes.md` | +38 |  |

7 more files, +232 −49

a line a page composed itself

| option | what it does | what happens | command |
|---|---|---|---|
| **approve** | builds the plan | its tasks run, a step at a time | `yunta resolve-gate 7E5PH4 approve` |
| **adjust** | Back to `plan` | the plan is written again with what you say<br>asks: what should change? | `yunta resolve-gate 7E5PH4 adjust --text "<answer>"` |

or

- `yunta close 7E5PH4`: closes it for good

| answer | on GitHub | what happens |
|---|---|---|
| **approve** | approve this pull request | the run goes on |
| **close** | close this pull request | nothing runs; this decision stays open |

on the machine that holds the run

- `yunta resume 7E5PH4`: reads the answer and acts on it

_the run holds its own state on disk._

- tokens: 20.2k spent

**next**

- `yunta status 7E5PH4`: where it stands

## on a screen

grill finished · brief finished · plan running · build never ran · review never ran · lint never ran · ship never ran

`↑↓` move · `enter` open · `n` new run · `x` close · `q` quit

◆ needs you: `yunta resolve-gate 7E5PH4 approve` (the plan waits for approval)

◆ **Q78GRR** needs you: the suite passed at baseline and now exits 101 · `a` go there

◆ needs you: node `implement` asks at its terminal

◆ **gate `ship` needs a decision** · approve KQERFD

the change is reviewed and its findings settled

- ▸ **approve** On to `pr`: opens the pull request
- **abort** Pause here: nothing runs

then pr, then the run finishes

`↑↓` move · `enter` chooses · `esc` leaves the run waiting

**plan** · A32BZY · reviewed-plan (default) · 2 tasks · needs you  approve-plan

Make it

you asked: a file the run is about

- the change

**steps**
- ▸ 1 T001
- T002

**impact**
- made.txt · T001
- said.txt · T002

**weighs**
- decided 2 `d`
- the suite is red at baseline
- flowchart LR
  n0["T001"]
  n1["T002"]
  n0 --> n1

**proof · T001** · step 1 of 1

tested by    the file exists  
**test -f made.txt**

◆ **Plan registered. Approve?**

`1` approve · `2` adjust · `3` abort

`j k` task · `p` the card · `esc` leaves the run waiting

**project name** (enter keeps `yunta`)

> yunta-tui

filter **n**

- **one-node** one command whose exit code is the check
- ▸ **lint-fix** a check, and an agent that fixes what it finds

`↑↓` move · `type` to filter · `enter` chooses · `esc` takes the first
