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
3. **Mapeo de sandbox** (el único mapeo NO probado en vivo): un nodo con
   `permissions: read-only` → la sesión de codex corre con
   `-s read-only` y un intento de escritura del agente falla dentro del
   sandbox de codex, no por el scope-check de yunta. Repetir con `edit`
   (`workspace-write`).
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
   y responde según sus instrucciones; un nombre sin definición falla el
   nodo antes de abrir la sesión, diciendo dónde la buscó.
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

Todo lo demás se construyó y testeó con mock/repos locales; estas tres
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

## E. Session hooks on the real CLIs (2026-10-06)

Run against codex-cli 0.157.0 and Claude Code 2.1.289 with a hook script that
logs its stdin and answers as told. Each item is what the engine relies on.

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
  `model`. A hook reaches a server on `127.0.0.1` under `workspace-write`.
- `--sandbox read-only --add-dir R` does not make `R` writable, for patches
  or the shell: a read-only session that must write its run's roots runs
  under `workspace-write`, with the hook refusing everything else.
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
