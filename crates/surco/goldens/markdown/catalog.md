# plan of run 7E5PH4

## ◆ fragua (standard) · needs you · for 32m10s · 7E5PH4

approve-plan asks you to approve the plan before anything is built.

The run plans the change, asks before it builds, and builds what you approve.

### what it found

- ✓ **git** commits as Ada <ada@example.com>
- ▲ **forge** github acme/web, reachable
- ◌ **review** no fan-out review

- ▲ A name with a newline in it breaks the line it is printed on.

- ▲ the budget sits under this workflow's usual spend
  - so: the run may stop before it finishes
  - fix: raise `limits.max_tokens_per_run`

### ✗ node `lint`: exit 101

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

A change **in Markdown**, with `code`.

- an item

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
| ✓ finished | `plan` | 4 tasks | 1m31s | 1.94M |

| | run | workflow | where it stands | |
|---|---|---|---|---|
| ◆ needs you | `EKPBGE` | fragua (standard) | implement asks at its terminal | 32m |
| ■ stopped | `60Y934` | fragua (quick) | stopped while it ran | `yunta resume 60Y934` |

a line a page composed itself

- **approve**: builds the plan  
  the four tasks run in three steps  
  `yunta resolve-gate 7E5PH4 approve`
- **adjust**  
  the plan is written again with what you say  
  asks: what to change  
  `yunta resolve-gate 7E5PH4 adjust --text "<answer>"`

or

- `yunta close 7E5PH4`: closes it for good

_the run holds its own state on disk._

- tokens: 20.2k spent

**next**

- `yunta status 7E5PH4`: where it stands
