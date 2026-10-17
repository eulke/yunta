# run 7E5PH4: ◆ needs you on a decision

| | node | |
|---|---|---|
| ✗ failed | `lint` | exit 101 |
| · never ran | `fix-lint` |  |

| step | task | proven by | |
|---|---|---|---|
| ▲ 1 | `cli-global` | no test runs its spec | Global and project flags for every pack command |
| 2 | `docs` | 1 spec test | The guide says where a pack installs |

```
line 4
line 5
line 6
line 7
line 8
error[E0425]: cannot find value `undefined_variable_name` in this scope, reported at src/lib.rs:3:5 by the compiler
```

3 lines above · whole output: `~/.yunta/runs/01K3W48MFW7H0ZZA5PZ07E5PH4/objects/9f2c`

- **retry** — runs `fix-lint` once more  
  one more correction attempt beyond `max_reroutes`  
  `yunta resolve-gate 7E5PH4 retry`
- **adjust**  
  the run goes on with what you say  
  asks: what to change  
  `yunta resolve-gate 7E5PH4 adjust --text "<answer>"`

- progress: nodes 1/2 · 1 failed
- tokens: 20.2k spent · 3 past runs, median 18k tokens

- `yunta status 7E5PH4` — where it stands
- `yunta close 7E5PH4` — closes it for good
