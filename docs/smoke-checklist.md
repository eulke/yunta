# Smoke tests en vivo pendientes (DI-22)

Checklist ejecutable para la primera sesión con credenciales y binarios
reales — lo que este sandbox no tiene (ni `codex` instalado, ni
token+repo descartable de GitHub). No es opcional: T7.4 y T7.7 se
construyeron contra documentación y fuente real, y estas corridas son su
✓ de aceptación diferido.

**Protocolo común**

- Cada divergencia encontrada se corrige **en su propia tarea**, jamás
  "de paso" en otra — con un test de regresión que la hubiera atrapado.
- El resultado de cada corrida (fecha, versión del binario, divergencias
  o "limpio") se registra en la entrada correspondiente de
  `docs/m0-status.md`, y al completar ambas se cierra DI-22 en
  `docs/deuda-implementacion.md`.

## A. `codex` real (T7.4)

Prerrequisitos: `codex` en el PATH (`codex --version` responde), sesión
autenticada (`codex login status`).

1. **Probe**: `yunta doctor` en un repo con `runners:` apuntando a
   `codex` → el probe reporta la versión, sin error.
2. **El workflow de 3 nodos de T7.3, contra codex**: en un repo de
   juguete (git init + commit), `.yunta/config.yaml` con
   `runners: { executor: [{ adapter: codex, model: <modelo vigente> }] }`
   y el workflow plan→implement→verify de la entrada T7.3 de
   `m0-status.md` (o el equivalente mínimo: un `prompt` que escribe un
   archivo y un `bash` que lo verifica). Correr
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
   `kill -9` al engine), `yunta resume <run>` → hoy re-corre el nodo
   (`restart_node`); cuando DI-23 esté cableado, repetir con
   `on_interrupt: resume_session` y verificar que
   `codex exec resume <thread_id>` continúa la MISMA conversación.

## B. `GitHubForge` real (T7.7)

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
4. **SHA drift (T7.7)**: repetir 1–3 pero, tras aprobar, pushear un
   commit más al branch del PR antes del `resume` → la aprobación vieja
   NO vale: el gate se re-abre/re-consulta en vez de darse por aprobado.
5. **Cambios pedidos**: un PR con review "request changes" → `resume` →
   el gate mapea a su re-ruta declarada (opción `on:`) o pausa citando
   el estado, según lo declarado.
6. **Degradación sin credenciales**: quitar la env var del token y
   `yunta run` de nuevo → degrada a consola con evento
   (`capability_degraded`/pausa explicando), jamás un crash ni un
   silencio (D66).
