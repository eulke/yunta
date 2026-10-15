# Smoke tests en vivo pendientes

Checklist ejecutable para la primera sesión con credenciales y binarios
reales — lo que este sandbox no tiene (ni `codex` instalado, ni
token+repo descartable de GitHub). No es opcional: el adapter de codex
y la integración con `GitHubForge` se construyeron contra documentación
y fuente real, y estas corridas son su ✓ de aceptación diferido.

**Protocolo común**

- Cada divergencia encontrada se corrige **en su propia tarea**, jamás
  "de paso" en otra — con un test de regresión que la hubiera atrapado.
- El resultado de cada corrida (fecha, versión del binario, divergencias
  o "limpio") se registra en `status.md`; al completar todas, el
  pendiente se cierra ahí.

## A. `codex` real

Prerrequisitos: `codex` en el PATH (`codex --version` responde), sesión
autenticada (`codex login status`).

1. **Probe**: `yunta doctor` en un repo con `runners:` apuntando a
   `codex` → el probe reporta la versión, sin error.
2. **El workflow de 3 nodos plan→implement→verify, contra codex**: en un
   repo de juguete (git init + commit), `.yunta/config.yaml` con
   `runners: { executor: [{ adapter: codex, model: <modelo vigente> }] }`
   y un workflow mínimo plan→implement→verify (un `prompt` que escribe
   un archivo y un `bash` que lo verifica). Correr
   `yunta run wf.yaml` (la vista viva es el default sobre una
   terminal). Verificar contra el log (`yunta status <run>`, `events.jsonl`):
   - `agent_session_opened` con `session_id` = el `thread_id` real y
     `model` = el pedido: el stream de codex no lo reporta
     (`openai/codex#14736`, sigue así en 0.157.0) y el adapter informa el
     modelo con el que lanzó la sesión.
   - `agent_message` de tipo `tool_use` con digests: `sha256:` y doce
     dígitos, nunca el comando ni el path (el mismo digest para dos
     llamadas idénticas, distinto para dos distintas).
   - `Usage` con `input_tokens`/`output_tokens`/`cached_input_tokens`
     reales (nombres de campo literales — cualquier rename del CLI
     rompe `parse.rs` y se ve acá).
   - Cierre `Completed` y el nodo `verify` en verde.
3. **Mapeo de sandbox** (las claves del perfil se midieron en la sección G; esta
   corrida prueba el argv que arma el adapter): un nodo con `permissions: read-only`
   → la sesión de codex corre con su perfil `yunta-session`, sin `--sandbox`, y un
   intento de escritura del agente en el checkout falla dentro del sandbox de codex,
   no por el scope-check de yunta. Repetir con `edit` (perfil sobre `:workspace`).
4. **Resume**: matar el engine a mitad de sesión (Ctrl-C dos veces o
   `kill -9` al engine), `yunta resume <run>` → re-corre el nodo
   (`restart_node`, el default). Repetir con `on_interrupt:
   resume_session` (ya cableado) y verificar que
   `codex exec resume <thread_id>` continúa la MISMA conversación —
   mismo `session_id` en el segundo `agent_session_opened`, y el agente
   retiene lo dicho antes del corte.

## B. `GitHubForge` real

Prerrequisitos: repo descartable en GitHub (con permiso de admin), token
fine-grained con `contents: rw` + `pull_requests: rw` en una env var, y
`forge: { github: { repo: <owner/name>, token_env: <VAR> } }` en la capa
usuario.

1. **Publish**: workflow con un `kind: gate` `external: { kind:
   pull_request, artifacts: [spec.md], branch: "{{run.branch}}" }` (el
   nodo previo produce `spec.md`). Correr → verificar: branch
   `yunta/<run_id>` pusheado con el artifact commiteado, PR abierto, el
   log con `gate_waiting.external_ref` = la URL del PR, y el run pausado.
2. **Poll pendiente**: `yunta resume <run>` sin tocar el PR → sigue
   pausado, sin re-publicar (ni branch nuevo ni PR duplicado).
3. **Approve → poll**: aprobar el PR en la UI con otra cuenta (o la
   misma, si el repo lo permite) → `yunta resume` → `gate_resolved` con
   `resolved_by` = el login del reviewer y `approved_sha` = el head del
   PR; el DAG continúa.
4. **SHA drift**: repetir 1–3 pero, tras aprobar, pushear un
   commit más al branch del PR antes del `resume` → la aprobación vieja
   NO vale: el gate se re-abre/re-consulta en vez de darse por aprobado.
5. **Cambios pedidos**: un PR con review "request changes" → `resume` →
   el gate mapea a su re-ruta declarada (opción `on:`) o pausa citando
   el estado, según lo declarado.
6. **Degradación sin credenciales**: quitar la env var del token y
   `yunta run` de nuevo → degrada a consola con evento
   (`capability_degraded`/pausa explicando), jamás un crash ni un
   silencio.

## C. `claude-code` — deltas posteriores a la sesión en vivo original

El adapter se probó en vivo en su momento, pero estas superficies se
construyeron después, contra documentación sola — merecen su corrida
tanto como codex. Prerrequisitos: `claude` en el PATH, sesión
autenticada.

1. **Staging de skills**: un nodo con `skills: [mi-skill]` y el
   skill en `.yunta/skills/mi-skill/` → verificar que la sesión ve el
   skill (el symlink aparece en `<worktree>/.claude/skills/mi-skill` y
   el CLI lo descubre — pedirle al agente que lo invoque), que el
   scope-check NO reporta `.claude/skills` como trabajo del agente, y
   que un segundo intento del nodo re-staged sin error (el symlink se
   reemplaza), y que al terminar la sesión el symlink ya no está.
2. **`agent:` a nivel nodo**: un nodo con `agent: <nombre>` y su
   definición en `.yunta/agents/<nombre>.md` → la sesión corre con
   `--append-system-prompt-file` (en `codex`, `-c developer_instructions`)
   y responde según sus instrucciones; un nombre sin definición rechaza el
   run antes de crearlo, diciendo dónde la buscó.
3. **`resume_session`**: mismo caso que A.4 pero con
   claude-code: matar el engine a mitad de sesión, `on_interrupt:
   resume_session`, `yunta resume` → `claude --resume <session_id>`
   continúa la MISMA conversación (mismo `session_id` en el log; el
   agente retiene contexto previo). Degradación: borrar el
   `session_id` del historial local del CLI (o correr en otro
   `$HOME`) → degrada a `restart_node` con `capability_degraded`/
   warning, jamás cuelga.
4. **`adapter_settings` passthrough**: declarar en config
   `adapters: { claude-code: { adapter_settings: {...} } }` con una
   clave que el CLI honre → verificar que llega (comportamiento
   observable o flag en el spawn), y que una clave desconocida no
   rompe el spawn.

## D. Superficies live-only agregadas en fases posteriores

Todo lo demás se construyó y testeó con mock/repos locales; estas cuatro
partes son las únicas que ninguna corrida sin credenciales puede
ejercitar.

1. **`yunta mcp` montado en Claude Code real**: agregar la
   config JSON de referencia (`mcpServers: { yunta: { command: yunta,
   args: [mcp] } }`) a una sesión real de Claude Code → pedirle al
   agente que liste workflows (`list_workflows` refleja el catálogo
   vivo, packs incluidos), dispare `run_workflow` (retorna `run_id` en
   milisegundos, el run corre desacoplado), consulte `workflow_status`
   y resuelva un gate con `resolve_gate`. Matar la sesión MCP a mitad
   de run → el run sigue corriendo de todos modos; `workflow_status`
   desde una sesión nueva lo confirma.
2. **MCP por-run con agente real**: un nodo `prompt` con
   claude-code real dentro de un grupo `coordination: blackboard`,
   pidiéndole al agente en el prompt que reporte un hallazgo con
   `yunta_post_finding` → verificar que el endpoint por-sesión llega
   montado al CLI real (la traducción adapter-specific se construyó
   contra documentación), que el `finding_posted` queda en el log
   atribuido al nodo, y que `yunta_get_blackboard` antes del join
   devuelve solo lo propio de ese grupo.
3. **`pack add` contra un host remoto real**: `yunta pack add
   github.com/<owner>/<repo>@<tag>` con un repo público real → el
   shorthand `host/path` expande a `https://`, el clone-por-ref y el
   vendoring funcionan igual que con los repos locales de los tests;
   `yunta pack list` reporta `ok` contra el lock recién escrito.

4. **Pedido bajo `deny` contestado en la llamada, en los dos CLIs**: una tarea de un
   `loop` con `scope_expansion` en `mode: deny` pide `src/other.rs` con
   `yunta_request_scope_expansion` y escribe ese path desde la shell en la misma
   sesión. La respuesta de la llamada llega en el momento, empieza con
   `request denied` y dice la razón; el log muestra `scope_expansion_requested` y
   `scope_expansion_denied` en esa llamada, antes de que la sesión pare. Después la
   sesión para con `stop_refused` (`outside_scope`), borra el archivo y termina. La
   secuencia es la misma en `claude-code` y en `codex`.

## E. Session hooks on the real CLIs (2026-10-06)

The scripted CLIs the contract suite drives report Claude Code 2.1.296 and
codex-cli 0.157.0, the versions this checklist pins. The items below were
checked against Claude Code 2.1.289 and codex-cli 0.157.0, with a hook script
that logs its stdin and answers as told; that flag registry stays as history.
A live run against these versions closes the claim. Each item is what the
engine relies on.

**codex**

- Hooks passed with `-c 'hooks.<Event>=[…]'` load in `codex exec`. They are
  skipped until trusted; the trust can be seeded in the same call with
  `-c 'hooks.state={"/<session-flags>/config.toml:<event>:<group>:<handler>"={trusted_hash="sha256:…"}}'`,
  so `--dangerously-bypass-hook-trust` is not needed. The hash is the SHA-256
  of the compact, key-sorted JSON of
  `{event_name, matcher?, hooks: [{type, command, timeout, async}]}`; it was
  reproduced for an existing user hook.
- Hooks in a checkout's own `.codex/hooks.json` do not run there (the
  checkout is not a trusted project).
- `PreToolUse` for an edit names the tool `apply_patch`; `tool_input.command`
  is the patch, its paths relative to `cwd` (`*** Add File: b/c.txt`). Exit 2
  with a message on stderr refuses the patch, and the model reads the message
  verbatim. The `--json` stream shows nothing for a refused patch.
- Every hook gets `session_id` (equal to `thread.started.thread_id`) and
  `model`. Under `--sandbox workspace-write`, a hook reaches a server on
  `127.0.0.1`.
- Under the `--sandbox` flag, which a confined session no longer passes (section
  G names the profile that replaced it), `--sandbox read-only --add-dir R` does
  not make `R` writable, for patches or the shell: a read-only session that must
  write its run's roots ran under `workspace-write`, with the hook refusing
  everything else.
- `Stop` answering `{"decision":"block","reason":…}` makes the session go on
  within the same turn; the next `Stop` gets `stop_hook_active: true`. The
  stream still ends with a single `turn.completed`.
- A `PostToolUse` hook that rewrites the patched file makes the next patch
  on stale context fail once (`apply_patch verification failed`); the model
  reads the file again and retries.
- A skill under `<cwd>/.agents/skills/<name>/SKILL.md` is found and used.
  `-c model_reasoning_effort="high"` is accepted, and so are `"low"` and
  `"xhigh"` on `gpt-6-luna`; `claude -p --effort xhigh` answers too.
- Through the engine: a `codex` session scoped to `src/**` told to write
  `src/a.txt` and `docs/b.txt` wrote the first; the second was refused by the
  engine before it applied, recorded as `write_refused` naming `docs/b.txt`,
  and the model quoted the refusal.
- Through the engine: a node scoped to `a.txt`, told to write `b.txt` from the
  shell and finish, asked before stopping; the engine found `b.txt` outside
  the scope, recorded `stop_refused` (`outside_scope`), and the same session
  removed `b.txt` and finished — the `Stop` hook trusted by its seeded hash.
- Through the engine: with `format: [{ paths: ["*.txt"], run: upper }]`, both
  patches to `notes.txt` were formatted (12 ms each, `write_formatted`), and the
  file ended `HELLO`/`BYE` — the `PostToolUse` hook trusted by its seeded hash.

**claude-code**

- `--setting-sources "" --strict-mcp-config` keeps OAuth working and drops
  what the user installed — their extensions, synced skills, MCP connectors
  and hooks; what Claude Code bundles remains. `--bare` is not usable: it is not logged
  in, and it runs no hook.
- `--settings` hooks run under both modes. A `sandbox` block
  (`enabled`, `failIfUnavailable`, `allowUnsandboxedCommands: false`,
  `network.allowLocalBinding: true`) lets Bash write the working directory
  and `--add-dir` roots, refuses `~`, refuses the internet, reaches
  `127.0.0.1`, and lets `cargo build` run.
- `Stop` with `decision: block` and `CLAUDE_CODE_STOP_HOOK_BLOCK_CAP=0`:
  the session goes on, `stop_hook_active` is `true` the next time, and a
  single `result` line closes it.
- After a `PostToolUse` rewrite with `additionalContext`, Claude reads the
  file again and its next `Edit` applies.
- Each assistant line repeats its message's `usage` once per content block.
  Counted once per `message.id`, `input_tokens`, `cache_read_input_tokens`
  and `cache_creation_input_tokens` add up to the `result` totals exactly;
  `output_tokens` is only complete in `result`.
- `--add-dir` is variadic: the prompt goes on stdin, never after it.
- With `allowedDomains: ["*"]` and `allowLocalBinding: true` a sandboxed
  command reaches the internet and `127.0.0.1`; with neither it reaches
  neither. `filesystem.denyWrite` on the working directory keeps a command
  from writing it while `--add-dir` roots stay writable.
- Through the engine: the same `format:` rule formatted both edits of
  `notes.txt` (13 ms each); the model said the hook uppercased the file, and
  its second edit applied to the formatted text.
- Through the engine: the same `outside_scope` stop as on `codex` — one
  `stop_refused`, `b.txt` removed by the same session, the node finished.
- Through the engine: an `edit` session under `network: false` wrote the
  checkout from Bash, was refused `example.com` by the sandbox's proxy, and
  could not write `~`.
- Through the engine, the same session's usage arrived per message (116k
  and 117k read, cache included) and the result added the 868 output tokens
  the messages had not counted: 233k in, 98% of it cached, where the same
  session had counted 770 tokens before.

**macOS**

- The first run of a freshly linked binary takes 310–500 ms (24 MB: ~500 ms);
  later runs take 5–10 ms. The suite runs 229 test binaries in sequence, and
  a suite whose binaries were linked again spends ~200–230 s outside any test.
- `yunta doctor` links a probe with `cc` and says so: 307 ms first run, 4 ms
  the next, here. Whether the Developer Tools exemption brings the first run
  down to the next, and what a suite then spends outside its tests, is still
  to be measured on a terminal a person exempts.
- The suite after touching `yunta-core`, 228 test binaries: rebuilding takes
  56 s; the first run 492 s, of which the tests take 252 s; the next run 246 s,
  of which the tests take 244 s. The first run's assessment is about 240 s; the
  binaries' own start costs about 2 s, and their links fit inside the 56 s
  rebuild — under a fifth of the suite once the assessment is gone, so the
  integration tests stay one binary per file.

## F. Whole runs, before and after the session rail (2026-10-09)

Fragua's `quick` mode on one idea ("add pack global installation support, not
only project level as today"), every run stopped at `ship`. Claude Code runs
the roles on `claude-opus-4-8`, `claude-sonnet-4-6` and `claude-haiku-4-5`;
Codex on `gpt-6-luna` for all of them. "Before" is the binary at the commit
the rail started from; "after" carries it.

| run | person stops | lint | stops refused | writes refused / formatted | working | rails | checks |
|---|---|---|---|---|---|---|---|
| Claude before (4 earlier runs) | 4–5 each; 1 of 4 reached `ship` | red in all 3 that ran it | — | — | — | — | — |
| Claude before | questions, `retry` blocked tasks, `retry` lint | red twice, 2 `fix-lint` sessions | — | 2 / — | 1h22m | — | 1h03m |
| Claude after | `continue-work`, `grant` | green first time | 0 | 4 / 71 | 1h29m | 3 s | 38m |
| Claude after | questions, `continue-work`, `retry` (account limit, before `awaiting_reset`) | green first time | 1 | 3 / 110 | 1h16m | 10 s | 42m |
| Codex before | `continue-work` | green first time | — | 0 / — | 7m | — | 43m |
| Codex after | `grant` ×3 | green first time | 1 | 7 / 31 | 23m | 1 s | 26m |

- La columna `rails` es el tiempo que `stats` le dio al formateo de cada run, citado
  aquí en segundos enteros. Dividirlo entre los formatos, la cuenta después de la
  barra en `writes refused / formatted`, da un promedio por escritura redondeado al
  segundo, no un p50. El p50 sale de los `duration_ms` de los eventos
  `write_formatted` del run: pásale un payload por línea a
  `jq -s '[.[] | select(.kind=="write_formatted") | .duration_ms] | sort'` y lee el
  elemento del medio de la lista ordenada.

- Un límite de cuenta que nombra su hora de reset pausa como `awaiting_reset` y
  retoma a esa hora, sin un `retry` de una persona; el `retry` de una persona se
  aplica a un límite solo cuando no nombró hora de reset. La parada `retry
  (account limit)` de la fila de arriba se hizo con el binario anterior a ese
  cambio.
- Parity between the CLIs is a test, not an observation: the contract suite
  (`cargo test -p yunta-adapters --test contract`) runs each case against
  claude-code, codex and the mock. The runs above saw the same causes give the
  same events on both CLIs: `stop_refused` (`criteria_red`,
  `artifact_missing`, `outside_scope`), `write_refused` naming the path,
  `write_formatted`, and each node's fence coverage.
- The rail costs 0.06–0.2% of the time sessions work. `fmt` never ran red;
  the held lint kept every task's work clean, so the `lint` node passed the
  first time in every run after.
- A refused stop sent the session back to work it would have left: a planner
  whose tasks document had not been taken tried four more times before it
  stopped.
- What still stops for a person after: a scope a plan left too narrow (each
  grant opens one more attempt), a scope request the loop's rules deny — the
  session may stop while its request waits, and the denial blocks the task
  instead of reaching the session — and an account limit. The denial reaches
  the session in its tool reply now, at once: item 4 of section D checks it live.
- A `read_only` Codex session cannot run `/usr/bin/git`: the Xcode shim writes
  a cache to the temporary directory, which Codex's `read-only` sandbox
  refuses, and a reviewer took the errors for a failed diff. Claude Code's
  `read_only` sandbox only refuses the checkout, so the same reviewer reads the
  diff there. That run used `--sandbox read-only` for read-only sessions; Codex
  now names its own profile for a confined session, which leaves the temporary
  directory writable and the checkout not (section G).
- Machine load differed between runs — up to three ran at once on 8 cores —
  so `checks` compares only runs that ran side by side.

## G. Live measurements: sandboxes, hooks, limits and the rail's cost (2026-10-11)

Claude Code 2.1.296, its sessions on `claude-haiku-5-5`; codex-cli 0.157.0,
its sessions on `gpt-6-luna` at low effort (the account refuses the
configured `gpt-6.1-sol`). Each live session ran in a throwaway repository.

**Codex permission profiles in `codex exec`.** A profile is passed with
`-c default_permissions=<name>`,
`-c permissions.<name>.extends=":read-only"`,
`-c 'permissions.<name>.filesystem={":tmpdir"="write",":slash_tmp"="write","<checkout>"="read"}'`
and `-c permissions.<name>.network.enabled=false`, without `--sandbox`.

| profile | checkout | temp | root | `.codex/x` | `git status` | network |
|---|---|---|---|---|---|---|
| read-only, no roots | refused | written | refused | refused | 0 | closed |
| read-only, one root | refused | written | written | refused | 0 | closed |
| edit (checkout `write`, `.codex` and `.git` `read`) | written | written | refused | refused | 0 | closed |
| full (`danger-full-access`) | written | written | written | written | 0 | open |

- A nested `read` wins over the checkout's `write`.
- `:slash_tmp` covers the whole `/private/tmp` tree: anything there is
  writable unless a nested entry names it.
- A shell write the sandbox refuses leaves no item in the `--json` stream:
  no event carries the denial, and the status `sandbox_denied` never appears.

**Codex hooks.** Every hook's stdin carries `session_id` and `model`. Hooks
passed with `-c hooks.<Event>` and trusted by hash in `-c hooks.state` run; an
untrusted hook does not run, and the stream says nothing about it. A `Stop`
hook that blocks twice with two different reasons keeps the turn going after
each block: each reason reaches the model as a user message, and the stream
carries no event for a block.

**Claude Code's sandbox.** A `Bash` write the sandbox refuses comes back as a
`tool_result` with `is_error` and the shell's own `operation not permitted:
<path>`, the same shape as any failing command; `PreToolUse` runs before the
sandbox and knows nothing of it. Each key applied in a `-p` session:
`filesystem.allowWrite` (temp, a root), `filesystem.denyWrite` (the checkout,
and a protected tree inside an allowed root) and `network.allowLocalBinding`.
The read-only shape the adapter writes today (`denyWrite` on the checkout,
`network: {}`, roots as `--add-dir`) leaves temp and loopback binds refused.

**Account limits**, measured against a local stub that answers 429 (no model
call):

| CLI | 429 | channel | gave up after | what it says |
|---|---|---|---|---|
| Claude | reset headers | stream, exit 1 | 5 s, no retry | `API Error: Server is temporarily limiting requests (not your usage limit) · status code (no body)`, and `rate_limit_event` with `status: rejected` and `resetsAt` |
| Claude | no headers | stream, exit 1 | about 3 min, 10 retries | the same text; `rate_limit_event` without `resetsAt` |
| Claude | provider's error body | stream, exit 1 | about 3 min, 10 retries | `… · Your account has reached its usage limit.` |
| Codex | headers or none | stderr, exit 1 | 2.5 s | `ERROR: exceeded retry limit, last status: 429 Too Many Requests` |
| Codex | `usage_limit_reached` body | stderr, exit 1 | 2.5 s | `ERROR: You’ve hit your usage limit. Try again later.` |

- Claude names "usage limit" in every form, so its text cannot tell a usage
  limit from a throttle; `assistant.error: rate_limit` and
  `result.api_error_status: 429` can, and the reset comes only as
  `rate_limit_event.resetsAt`.
- Codex prints no reset time in any form.

**The rail and format on write**, 200 calls each through the release binary,
spawned the way a CLI runs its hook:

| call | p50 | p95 |
|---|---|---|
| `before-write` | 3.96 ms | 4.13 ms |
| `after-write`, formatting a 298-line file | 39.69 ms | 40.39 ms |
| `before-stop` | 4.20 ms | 4.35 ms |
| spawning `/usr/bin/true` | 1.92 ms | 2.05 ms |
| `sh -c true` | 3.91 ms | 4.08 ms |
| `rustfmt` through the rustup proxy | 30.70 ms | 31.37 ms |
| the toolchain's own `rustfmt` | 15.52 ms | 16.00 ms |
| the engine's command, `sh -c 'rustfmt --edition 2021 "$@"'` | 34.06 ms | 34.98 ms |

- The rustup proxy adds about 15 ms per write; the engine's whole command
  stays under 45 ms per write, so the project's format rule keeps running
  `rustfmt` from `PATH`.

## H. Whole runs after every rule got one owner (2026-10-15)

Fragua's `quick` mode on the idea of section F, each run stopped at `ship`, on
Claude Code 2.1.299 (`claude-opus-4-8` plans, `claude-sonnet-4-6` builds) and
codex-cli 0.157.0 (`gpt-6-luna`). Both started on `1430e120`. What a run or a
directed scenario found was fixed, and the runs went on on the fixed binary
from their person stops after 04:10 UTC (`f281105c`; `c61fef8e` and
`954356be` fixed what came after).

| run | person stops | lint | stops refused | writes refused / formatted | working | rails | checks |
|---|---|---|---|---|---|---|---|
| Claude | questions, `retry` (limit), `grant` ×2, `continue-work` ×3 | green first time | 1 | 7 / 127 | 1h05m | 7.1 s (0.18%) | 3h27m |
| Codex | `grant` ×7, `continue-work`, `send-back` ×2 | green first time | 2 | 13 / 110 | 41m | 6.6 s (0.27%) | 1h24m |

- **The rail's engine side**, from each session's `rail_summary` (p50 / p95):
  `before-write` 71 µs / 0.96 ms on Claude and 39 µs / 1.7 ms on Codex;
  `after-write` is the formatter, 45 / 98 ms and 45 / 106 ms; `before-stop` is
  the stop judgement, which runs the task's criteria when the tree changed,
  147 / 180 ms and 164 ms / 4.7 s. A formatted write takes 41 and 42 ms at p50.
  No `rail_failed` on either run.
- **Person stops.** Claude: the grill's questions, two grants of scope under
  `ask`, a `continue-work` after a fresh session also outgrew the context window
  and one after work that did not compile; the `retry` after an account limit
  and the first `continue-work` after a context that ran out are mechanical, and
  the engine now routes both itself. Codex: seven grants under `ask` (the plan's
  scope was narrow for its resolution task), and a departure answered with a
  `send-back`; its `continue-work` and a wordless `send-back` came from a parked
  menu that could not answer the departure, which it now can.
- **Tokens**: Claude 113M in (108M cached) and 344k out; Codex 58M in (56M
  cached) and 143k out. `cached ⊆ input` holds, and `status`, `stats` and
  `--json` give the same figure.
- **Engine words in the log.** Every string with spaces is a command, its
  output, an agent's or a CLI's words, or an input, but for three: the digest
  that stands in for an agent's note (`142 bytes, sha256 …`), the labels and
  trade-offs of an author's gate and of the budget menus (`` On to `pr` ``), and
  the title of the finding a scope denial posts. Failure and departure menus
  keep ids only; these still keep words.

**Directed scenarios.** Each ran in a throwaway repository on `1430e120`, with
the mock where the claim is the engine's and a real CLI where it is the CLI's,
and a second agent re-read each log.

| scenario | CLI | result |
|---|---|---|
| A `read_only` node with one root writes the checkout | Codex, Claude | the sandbox refuses it; `git status` exits 0; the network stays closed |
| The project denies `config/secrets.yaml` | Claude, Codex | the file tool's write is `write_refused` at once; the command's write fails the node at the close with `paths_denied`; the other file lands |
| An `edit` node under `sandbox: read-only` | Claude, Codex | nothing of the checkout is written; `confinement` says `worktree: nothing` |
| A session runs `git commit` | Codex, Claude | **found**: the commit lands when the checkout lies under the temporary directory, whose grant covers the repository's git directory |
| A scope request over `.github/workflows/ci.yml`, which the project denies | Claude, Codex | answered in about 10 ms; requested, denied and a finding on the log; the session goes on; nothing reaches a person |
| A project denies `**/*.lock`; a task asks for `src/new.rs` with red evidence | mock | granted, `decided_by: evidence` |
| `AGENTS.md` under `ask` and `deny`; `.claude/settings.json` | mock | `ask` owes a person; `deny` refuses at once; `.claude/settings.json` is refused whole under both |
| A task closes green with a request left | mock | `scope_expansion_withdrawn`; the node finishes |
| An account limit with a known reset (stub) | Claude | `awaiting_reset`, a detached resume at the reset, exit 5 |
| A Codex limit, which names no hour | Codex | `retry` once the service accepts requests; no `awaiting_reset` |
| A red check; a person fixes the tree | mock | `abort` only until the tree moves; then `retry` |
| A paused mock run resumes | mock | stays on the mock; takes the fixture's next script |
| An agent and a skill edited mid-run; a frozen skill altered by hand | Claude | the next session gets what the run was born with; the altered skill fails the node with `frozen_input` |
| A session that cannot be stopped | mock | `session_not_stopped` with its spend; `retry` once something outside the tree changes |
| A hung formatter (`sleep 6 #`) | Claude, Codex | `write_formatted` with `exit_code: -2` and `timed_out: true` at 5 s; the session goes on |
| Aborting a plan departure | mock | the run pauses without a missing answer; the question comes back on resume |
| A pending `ask` request with red criteria | mock | **found**: a person's grant from the parked menu opened a fresh session instead of the one that asked |
| A `rules` request a red output locates, by tool or by file | mock | the same grant either way |
| Git processes per judged capture and per handed-back checkout | mock | two `ls-files` per capture; one per give-back |

**What the runs and the scenarios found, each fixed through its owner:**

- **A pin was judged on nodes the run never reaches.** `--mode quick --adapter
  claude-code` was refused for a node `quick` leaves out, and a run probed
  adapters no node of it opens a session on. One owner now says which nodes a
  run reaches in its mode, and the start, the token cap, the plan's review and
  spec, `check` and the probe read it.
- **A session committed under a temporary directory** (s07, s08). Every
  session's sandbox now denies the run's git directory wherever it lies.
- **A person's grant opened a fresh session** (s23), and its label named no
  path. The task goes back to the session it worked in last, and the label
  names the task and its paths.
- **An account limit with a reset left the loop's tasks blocked** (Claude): the
  run woke at the reset and the loop failed at once with the spent limit, for a
  person; the wake also came a second early. The loop now reopens the tasks the
  limit stopped at the attempt after the reset, in the sessions it cut, and the
  wait asks the wall clock until the reset.
- **A departure owed with no terminal could not be answered** (Codex, s22): its
  parked menu offered `continue-work` and `abort`. It offers `accept` and
  `send-back`, and the send-back asks for the words the departure's own
  question asks for.
- **A call or a stop judgement its session's stop cut short read as an engine
  failure** (Claude: two sessions reached their 45-minute timeout while their
  check ran). Such a call records no refusal and no `rail_failed`.
- **A cancel sent the moment `--detach` returns found nothing to stop** while the
  run went on. It waits for the engine it was handed to to register.
- **A session whose conversation outgrew the context window stopped for a
  person** (Claude, twice on one task): Claude Code ended it with "Autocompact
  is thrashing", read as a final refusal. It is its own cause,
  `context_exhausted`, and the task's work goes to one fresh session; a second
  one stops the task. Codex's marker, `context window exceeded`, is the name its
  0.157.0 binary gives the error; no run has shown it yet.
- **Smaller:** `stats` names the model each node's sessions ran on (Codex's from
  the rail); a node open while its run is paused reads `paused`; each link of a
  chain of detached resumes adds to its log.

**Proven on the fixed binaries:**

- **The limit loop** (a stub answering 429 with a reset two minutes out): on
  `1430e120` the wake failed the loop at once and went to a person; on
  `f281105c` the wake came at the reset, reopened the task and, the stub still
  refusing, waited for the next reset with no person, both links of the
  detached chain in one `detached.log`. In the Claude run, a second limit woke
  at 07:40:00.079 and its task picked its work back up in the session the limit
  cut, with no person.
- **`git commit` from a session** prints `commit=128` on both CLIs, and the run
  branch holds the engine's commit with its `Yunta-Run` and `Yunta-Node`
  trailers.
- **s23 again**: the grant's label reads "Allow task `task-h` to also write
  b.txt", and the task resumes the session that asked; in the Claude run, the
  same.
- **`stats`** names `gpt-6-luna` for a Codex node and `claude-haiku-5-5` for a
  Claude one; the pinned `quick` launch is created, and its node reads `paused`
  once cancelled.
- **On `954356be`**: a cancel sent the moment `--detach` returned stopped the
  run in 1.3 s; a wordless `send-back` from a parked departure was refused with
  what it asks, and the worded one resumed the session that departed and
  finished the run.
