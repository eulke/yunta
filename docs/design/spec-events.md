# Spec — Payloads de eventos del event log

**Estado:** normativo v0.1 · **Alcance:** especificación campo por campo de cada
tipo de evento del event log, la política de versionado aplicada y la política de
`event_hash`. Precede a los tipos de Rust, igual que la spec del ledger precede al
parser del ledger.

> El Contrato del Run da la tabla evento→emisor→payload-relevante y las políticas de
> versionado y hashing, pero no el detalle campo por campo de cada payload. Ese
> detalle se fija acá, con cita de la fuente cuando existe y marcado
> **[inferido]** cuando no hay texto normativo literal que lo respalde.

## 0. Conteo de eventos

La tabla de eventos del Contrato del Run tiene 25 filas y **31 `kind` distintos**
(20 filas de 1 kind, 4 filas de 2 kinds y 1 fila de 3 kinds). La tabla es el
contenido normativo; este documento especifica esos 31 kinds tal como la tabla los
enumera.

## 1. Envelope común

Todo evento comparte la misma tupla persistida:

| Campo | Tipo | Notas |
|---|---|---|
| `run_id` | `RunId` (ULID) | identifica el run |
| `seq` | `u64` | orden monotónico dentro del run — define el orden de replay |
| `timestamp` | `DateTime<Utc>` | reloj inyectado (`Clock` trait, nunca `SystemTime::now()` directo) |
| `node_id` | `Option<NodeId>` | ausente para eventos de alcance run (`run_created`, `run_paused`, ...) |
| `kind` | string | uno de los 31 nombres de este documento, con su sufijo `_vN` si no es la v1 |
| `payload_json` | JSON | específico de cada `kind` — detallado más abajo, campo por campo |
| `schema_version` | `u32` | versión *del payload de ese kind*, no global — ver la política de versionado más abajo |

**El `node_id` vive solo en el envelope.** Ningún payload que corre en el contexto
de un nodo lo repite: `NodeStartedPayload`, `ArtifactWrittenPayload`,
`ContextAssembledPayload`, `ScopeCheckedPayload` (lleva solo `task_id` opcional),
`NodeFinishedPayload`, `NodeFailedPayload`, `HookExecutedPayload`,
`NodeReroutedPayload` (lleva solo `to_node`), `GateWaitingPayload`,
`GateResolvedPayload`, `QuestionsAnsweredPayload`, `LoopIterationPayload`,
`FindingPostedPayload`, `ChildRunCreatedPayload` y `ChildRunFinishedPayload` leen el
nodo del envelope. Un dato repetido en dos lugares puede desincronizarse; uno solo,
no.

Los siete campos de la tupla, **en este orden**, son también los que participan en
`event_hash`.

## 2. Política de versionado

Cuatro reglas, aplicadas desde el primer commit:

1. **Versión por tipo, no global.** `schema_version` versiona *ese* `kind`;
   `criteria_checked` puede estar en v3 mientras `run_created` sigue en v1.
2. **Dentro de una versión, solo cambios compatibles** (agregar campos opcionales).
   Renombrar, eliminar o cambiar el tipo de un campo existente es un **`kind`
   nuevo** (`criteria_checked_v2`), nunca una migración del log — el event log es
   append-only y una migración introduciría una operación falible sobre la
   fuente de verdad.
3. **Lector tolerante, escritor estricto.** Campos desconocidos se ignoran al leer;
   al escribir, el payload se valida contra el schema — un evento inválido es un
   bug del engine, no un warning.
4. **`kind` desconocido → run parcialmente interpretado, nunca `broken`.** Ninguna
   degradación es silenciosa: el run sigue interpretándose hasta donde puede, con
   el `kind` desconocido señalado explícitamente, nunca escondido. El lector
   conserva el evento entero — `kind`, `schema_version` y todos sus campos — y
   `status`, el recibo y `stats` cuentan los kinds desconocidos por nombre.

El JSON Schema se **genera desde los tipos de Rust** (fuente de verdad = código) y
se versiona en el repo; cualquier cambio de payload produce diff visible en PR. La
normalización a modelo de dominio ocurre en memoria al leer — **ningún `_vN`
aparece en `status`, `stats`, el recibo ni ninguna otra superficie de usuario**.

**Versión inicial de cada `kind`**: todos arrancan en **v1**. No hay
historial previo — es la primera implementación.

## 3. Política de `event_hash` (spec, no implementación)

> Implementado: `yunta-storage` calcula el hash en `append_event` y lo
> verifica en `Storage::verify_chain` / `yunta verify <run_id>`, siguiendo esta
> política al pie de la letra. La codificación concreta de los campos es
> length-prefixed (`len:bytes;`) para que ningún límite de campo sea ambiguo.

- **Fórmula**: `event_hash = SHA-256(prev_event_hash || campos_estructurales_en_orden_fijo)`.
- **Campos que participan**, en el orden fijo del envelope descrito arriba (no
  orden alfabético): `run_id, seq, timestamp, node_id, kind, payload_json,
  schema_version`.
- Se calcula sobre **los bytes exactamente como se persistieron**, antes de
  cualquier normalización de lectura — la integridad de la cadena es
  ortogonal a la evolución del schema.
- **Génesis**: `H0 = SHA-256(manifest_hash)` — determinístico, único por run, sin
  constante arbitraria.
- **Persistencia**: el hash se guarda junto al evento; nunca se recalcula on-demand
  en cada replay.
- **Verificación**: operación aparte y explícita — corre automáticamente al generar
  el recibo y bajo demanda vía `yunta verify <run_id>`. Una cadena rota (payload
  alterado, evento borrado/insertado/reordenado, o `prev_event_hash` alterado) marca
  el run `broken` con diagnóstico del punto exacto de ruptura.
- **Alcance declarado**: integridad y orden, **no autenticidad** — la firma
  criptográfica (deuda consciente, sin ADR de diseño todavía) es una capa
  aparte y deliberadamente separada.

## 4. Mapeo `AgentEvent` (Spec Adapter) → `kind` del event log

El trait `Adapter` emite 6 variantes de `AgentEvent` (`SessionOpened`, `ToolUse`,
`Usage`, `Note`, `Completed`, `Failed`), pero solo dos `kind` del event log están
atribuidos al adapter: `agent_session_opened` y
`agent_message`. El mapeo completo no está explicitado en ningún lado; se resuelve acá
**[inferido]**, apoyado en principios ya normativos:

| `AgentEvent` | `kind` resultante | Razón |
|---|---|---|
| `SessionOpened` | `agent_session_opened` | mapeo directo — obligatorio y siempre primero |
| `ToolUse` / `Usage` / `Note` | `agent_message` (con `message_type` distinguiendo) | es el único otro `kind` atribuido al adapter; `agent_message` es genérico ("resumen/uso de tokens") |
| `Completed` / `Failed` | **ninguno directo** — dispara la verificación del engine, que emite `node_finished` / `node_failed` | la palabra del agente nunca es evidencia: no existe verifier-agente, la verificación es fase del engine, no un evento que el adapter pueda producir por sí mismo; el contrato del adapter solo obliga a que el *stream* termine con uno de los dos, no que se persista tal cual |

Si esta lectura no es la intención original, es exactamente el tipo de cosa a
corregir con una nota tuya antes de que se convierta en tipos de Rust.

## 5. Los 31 tipos de evento, campo por campo

Convención de esta sección: **Fuente** cita la columna "Payload relevante"
tal cual está documentada; **Campos** expande eso a nombre/tipo/obligatoriedad/nota,
marcando `[inferido]` lo que no tiene respaldo textual directo.

---

### 5.1 `run_created` — engine
**Fuente:** manifest hash, inputs, modo, `promoted_from?`

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `manifest_hash` | string (hash) | sí | identifica el manifest congelado |
| `inputs` | mapa string→valor | sí | inputs resueltos y validados, defaults incluidos |
| `mode` | string | sí | nombre del modo elegido |
| `promoted_from` | `Option<RunId>` | no | presente solo si este run nace de una promoción |
| `yunta_schema` [inferido] | string (semver-range) | no | declarado o inferido del binario — congelado junto al resto |
| `base_branch` / `base_commit` [inferido] | string | sí | necesarios para el worktree y forman parte del manifest congelado |

### 5.2 `runner_resolved` — engine
**Fuente:** rol, candidato elegido, candidatos descartados y causa

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `runner` | string | sí | nombre del runner (`runner:`); el lector acepta también `role`, el nombre anterior del campo |
| `chosen` | `{adapter, model, agent?}` | sí | binding resuelto y congelado |
| `discarded` | lista de `{candidate, reason}` | sí (puede ser vacía) | candidatos no elegidos y por qué — nunca vacío sin motivo si hubo &gt;1 candidato |

### 5.3 `baseline_captured` — engine
**Fuente:** comando de suite, resultados, hash

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `command` | string | sí | `baseline.suite` resuelto de config |
| `results` [inferido] | `{exit_code, summary}` | sí | resultado crudo de correr la suite una vez al abrir el run |
| `hash` | string | sí | hash del resultado, insumo de `baseline_compare` |

### 5.4 `node_started` — engine
**Fuente:** node_id, intento N

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `attempt` | `u32` | sí | 1-indexado; sube con cada reintento |

### 5.5 `agent_session_opened` — adapter
**Fuente:** session_id, agente, modelo, capacidades

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `session_id` | `SessionId` (opaco) | sí | persiste para `resume` |
| `agent` | `Option<String>` | no | agente nombrado del adapter, si se pidió (`agent:`) |
| `model` | string | sí | modelo efectivamente usado |
| `capabilities` | `Capabilities` (bools: resume_session, edit_hooks, permission_profiles, custom_agents, usage_reporting, run_tools) | sí | snapshot de capacidades del adapter en ese momento — constantes tras construcción |

### 5.6 `agent_message` — adapter
**Fuente:** resumen/uso de tokens (nunca el texto completo)

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `message_type` [inferido] | enum `tool_use \| usage \| note` | sí | distingue cuál variante de `AgentEvent` originó el mensaje |
| `tool_name` [inferido] | `Option<string>` | solo si `tool_use` | de `ToolUse.name` |
| `target_digest` [inferido] | `Option<string>` | solo si `tool_use` | de `ToolUse.target_digest` — nunca contenido completo |
| `input_tokens` / `output_tokens` [inferido] | `Option<u64>` | solo si `usage` | de `Usage` |
| `cached_input_tokens` [inferido] | `Option<u64>` | no | opcional incluso dentro de `usage` — solo si el CLI distingue lectura de caché |
| `text` [inferido] | `Option<string>` | solo si `note` | resumen mecánico `N bytes, sha256 <prefijo>` del texto de `Note` — jamás el contenido: el log no debe poder portar un secreto que la nota contenía, así que el resumen es contenido-cero, no meramente acotado |

### 5.7 `artifact_written` — engine
**Fuente:** node_id, path, content hash

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `path` | string | sí | relativo a `run.dir/artifacts/` |
| `content_hash` | string | sí | los artifacts son inmutables; esto es lo que se verifica en `resume` |

### 5.8 `context_assembled` — engine
**Fuente:** node_id, fuentes resueltas, hash por segmento de estabilidad

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | no | presente cuando el ensamblado construyó el brief de UNA tarea dentro de un `loop` — la misma convención tarea-vs-nodo de `scope_checked.task_id`; ausente en el ensamblado de un nodo `prompt` |
| `sources` | lista de `{source_id, kind}` | sí | qué `ContextSource` se resolvieron |
| `segment_hashes` | mapa `stable \| run-stable \| volatile` → hash | sí | orden fijo estable→run-estable→volátil→prompt; insumo directo de replay/diff |

### 5.9 `task_registered` — engine
**Fuente:** task_id, criteria, scope, deps

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string (mismo patrón de id que en el schema del ledger) | sí | — |
| `criteria` | lista de `{cmd, type?}` | sí | copia congelada del ledger |
| `scope` | lista de globs | sí | — |
| `depends_on` | lista de `task_id` | no | default vacío |

### 5.10 `criteria_checked` — engine
**Fuente:** task_id, fase pre/post, exit code por criterio, ejecutado o reutilizado de caché

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `phase` | enum `pre \| post` | sí | pre-check en rojo vs. post-check |
| `results` | lista de `{cmd, exit_code, type?, reused: bool, duration_ms?}` | sí | `reused=true` cuando la memoización (fuera de alcance de una implementación completa, salvo lo mínimo necesario) sirvió el resultado sin re-ejecutar; `duration_ms` es el costo observado de la ejecución — ausente en `reused=true` y en eventos emitidos antes de que este campo se agregara |

### 5.11 `task_status_changed` — engine
**Fuente:** task_id, estado nuevo, evento que lo justifica

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `new_status` | enum `pending \| ready \| running \| done \| blocked \| failed` [inferido, valores exactos a confirmar contra la implementación del scheduler] | sí | solo el engine emite este evento — ningún agente tiene vía para marcarlo |
| `caused_by` | referencia a `seq` de otro evento | sí | el evento (p. ej. `criteria_checked`) que justifica la transición |

### 5.12 `scope_checked` — engine
**Fuente:** task_id/node_id, diff observado, violaciones

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | `Option<string>` | no | presente si el chequeo es de una tarea dentro de un `loop`; ausente para un chequeo de nodo suelto — el nodo en ambos casos es el `node_id` del envelope |
| `diff` | lista de paths | sí | de `git diff` contra el scope declarado |
| `violations` | lista de paths | sí (vacía si limpio) | paths fuera de todo glob declarado |

### 5.13 `scope_expansion_requested` — engine
**Fuente:** task_id, paths, razón, criterio propuesto y su pre-check

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `paths` | lista de globs | sí | ampliación pedida |
| `reason` | string | sí | — |
| `proposed_criterion` | `Option<{cmd}>` | no | si el agente propone además un criterio nuevo |
| `proposed_criterion_precheck` [inferido] | `Option<{exit_code}>` | solo si hay `proposed_criterion` | un criterio que ya pasa se rechaza automático sin consultar |

### 5.14 `scope_expansion_granted` / `scope_expansion_denied` — engine
**Fuente:** task_id, decisor (regla o persona), modo, conteo del run

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `decided_by` | enum `rule \| person` + identificador | sí | — |
| `mode` | enum `rules \| ask \| deny` | sí | modo vigente en el momento de la decisión |
| `count_this_run` | `u32` | sí | para el cap `max_per_run` |
| `denial_reason` | `Option<string>` | solo en `denied` | toda denegación produce además un `finding_posted` — no lo reemplaza, lo acompaña |

### 5.15 `node_finished` / `node_failed` — engine
**Fuente:** resultado, tokens, ¿reintentable?

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `outcome` [inferido] | dato del engine tras verificación, no el `AgentOutcome` crudo del adapter | solo en `node_finished` | el outcome del agente es telemetría, esto es el veredicto |
| `failure` | `{outcome}` \| `{artifacts}` | solo en `node_failed` | por qué falló, como dato; ver abajo |
| `tokens_used` | `{input, output, cached?}` | sí | acumulado desde `Usage` |
| `retryable` | `bool` | solo en `node_failed` | guía la política de reintento; lo fija quien gobierna el presupuesto, de modo que un intento terminal nunca se registra como reintentable |

**La falla es dato, no prosa.** `failure` toma una de dos formas, planas sobre el
payload: `outcome: <frase>`, una falla que el engine enuncia en una oración, o
`artifacts: [...]`, un elemento por artifact declarado que no cerró. Cada elemento
es o el archivo — `path` y uno de `artifact-missing`, `artifact-empty`,
`artifact-oversized` (con bytes y techo) o `artifact-unreadable` — o el contenido:
el path del documento, la `kind` que fija su forma y todos sus diagnósticos. El
texto que ve una persona se produce al leer el evento, nunca al escribirlo (D133).
Un payload que lleva `outcome:` solo se lee como la falla de una frase, sin
migración: es la tolerancia de lectura de §3.1 del Contrato aplicada a este campo.

### 5.16 `hook_executed` — engine
**Fuente:** node_id, fase before/after, comando, exit code

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `phase` | enum `before \| after` | sí | hooks son ciclo del engine, no del adapter |
| `command` | string | sí | — |
| `exit_code` | `i32` | sí | — |

### 5.17 `node_rerouted` — engine
**Fuente:** nodo fallido, destino, causa, reintento N de M

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `to_node` | `NodeId` | sí | destino — el nodo que reruteó es el `node_id` del envelope |
| `cause` | string | sí | — |
| `origin` | enum `on_failure \| gate_choice` | no (default `on_failure`) | qué mecanismo reruteó; logs viejos sin el campo leen `on_failure` |
| `attempt` | `Option<u32>` | solo en `on_failure` | N de `max_reroutes` (M); ausente en una elección de gate, que no es un reintento |
| `max_reroutes` | `Option<u32>` | solo en `on_failure` | — |

### 5.18 `gate_waiting` / `gate_resolved` — engine/adapter
**Fuente:** opciones, elección, quién, feedback

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `summary` | string | solo en `gate_waiting` | objeto de escalación |
| `evidence` | lista de `{label?, value}` | solo en `gate_waiting` | la adjunta el engine desde el log; nunca prosa generada por agente. Un hecho que se nombra solo (`exit 1`) no lleva `label`. Un log anterior a la estructura trae un string y se lee como el único hecho sin etiqueta que siempre fue |
| `options` | lista de `{id, label, tradeoff}` | solo en `gate_waiting` | `tradeoff` es obligatorio por opción |
| `chosen_option` | `Option<string>` | solo en `gate_resolved` | — |
| `resolved_by` | `Option<string>` | solo en `gate_resolved` | usuario o identificador de quien resolvió |
| `free_text` | `Option<string>` | no | siempre disponible como canal |

### 5.19 `questions_answered` — engine
**Fuente:** node_id, hash del artifact de respuestas, canal (tty\|mcp\|pr), respondiente si se conoce

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `answers_hash` | string | sí | hash del artifact de respuestas |
| `channel` | enum `tty \| mcp` | sí | — |
| `responder` | `Option<string>` | no | si el canal lo identifica |

### 5.20 `loop_iteration` — engine
**Fuente:** iteración N, evaluación de `until`

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `iteration` | `u32` | sí | — |
| `until_result` | `bool` | sí | resultado de evaluar la condición del loop — la evalúa el engine, no el agente |

### 5.21 `finding_posted` — engine
**Fuente:** autor (nodo o engine), hallazgo: id, severidad, título, location, detalle

El engine también es autor: cada degradación que sufre —un `git` de
distill que falla, una limpieza que no corre, su propio `engine.json` no
escribible— se registra como un `finding_posted` con `node_id` ausente
(hallazgo de run) e `id` compuesto por el engine, nunca un `warn` que
deje el log en silencio.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `finding.id` | string | sí | autor = `node_id` del envelope; ausente cuando lo emite el engine (hallazgo de run) |
| `finding.severity` | enum | sí | usada por `findings_gate`; las degradaciones del engine son `minor` |
| `finding.title` | string | sí | — |
| `finding.location` | string | sí | usada para deduplicación |
| `finding.detail` | string | sí | — |
| `finding.proposed_criterion` | `Option<{cmd}>` | no | — |

### 5.22 `promotion_signaled` — engine
**Fuente:** razón, evidencia, modo sugerido

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `reason` | string | sí | la afirmación y los hechos detrás, en una línea — es el único campo que un lector del evento en sí recibe |
| `evidence` | lista de `{label?, value}` | sí | la misma estructura que `gate_waiting`, con la misma tolerancia de lectura |
| `suggested_mode` | string | sí | debe respetar la escalera de promoción |

### 5.23 `child_run_created` / `child_run_finished` — engine
**Fuente:** node_id, child run_id, `workflow_hash` del hijo, estado terminal

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `child_run_id` | `RunId` | sí | referencia histórica inmutable — el nodo `kind: workflow` del padre es el `node_id` del envelope |
| `child_workflow_hash` | string | sí | fija qué versión del workflow hijo corrió — reproducir el padre nunca resuelve una versión nueva |
| `terminal_state` | estado | solo en `child_run_finished` | — |
| `tokens` | `TokenUsage` | solo en `child_run_finished` | el gasto total derivado del hijo a su cierre — el Usage de los hijos agrega hacia arriba: el replay del padre lo suma exactamente una vez por miembro de cadena; el `node_finished` del nodo `workflow` deliberadamente no lleva tokens del hijo (doble conteo) |

### 5.24 `capability_degraded` — engine
**Fuente:** capacidad, adapter, política aplicada

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `capability` | string (nombre de campo de `Capabilities`) | sí | — |
| `adapter` | string (`id()` del adapter) | sí | — |
| `policy_applied` | string | sí | de la tabla de degradación de capacidades del adapter, o —cuando el listener MCP de `run_tools` no puede abrir— el texto que dice que la sesión corre sin run tools y por qué |

### 5.25 `run_paused` / `run_resumed` / `run_finished` — engine
**Fuente:** razón / estado terminal, métricas

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `reason` | string | solo en `run_paused` | presupuesto excedido, gate esperando, etc. |
| `resume_policy_applied` [inferido] | `Option<string>` | solo en `run_resumed` | el único `on_interrupt` que todos los huérfanos resolvieron; ausente sin huérfanos o con políticas distintas |
| `policies` | lista de `{node, on_interrupt}` | solo en `run_resumed` (puede ser vacía) | cada nodo que el log dejó `running` sin evento terminal y la política a la que resolvió: la propia o el default de la config |
| `terminal_state` | estado | solo en `run_finished` | — |
| `metrics` | `{cptv?, tokens, ...}` | solo en `run_finished` | derivadas del log, nunca estimadas |

## 6. Regla transversal

Ningún payload de ningún `kind` de este documento puede contener: contenido completo
de archivo, prompt o respuesta del agente, ni valores de secretos. Donde el payload
necesita referenciar contenido, lleva un hash o un resumen acotado — nunca el dato
en sí. El engine redacta además cualquier valor de secreto conocido antes de
persistir, como defensa en profundidad — no como excusa para ser menos
cuidadoso en el diseño del payload.
