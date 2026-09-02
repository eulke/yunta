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
   `yunta run wf.yaml --follow`.
   Verificar contra el log (`yunta status <run>`, `events.jsonl`):
   - `agent_session_opened` con `session_id` = el `thread_id` real y
     `model` = el pedido (el stream de codex no lo reporta —
     `openai/codex#14736`; confirmar si sigue así).
   - `agent_message` de tipo `tool_use` con digests coherentes
     (`command_execution` → el comando; `file_change` → un path).
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
   reemplaza).
2. **`agent:` a nivel nodo**: un nodo con `agent: <nombre>`
   de un agente definido en el repo → la sesión corre con `--agent
   <nombre>` y responde con la persona correcta; un nombre inexistente
   → el error del CLI llega como fallo del nodo con diagnóstico, no
   como cuelgue.
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
