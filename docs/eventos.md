# Spec — Payloads de eventos del event log (T2.0)

**Estado:** normativo v0.1 (nuevo — no existía como documento separado en Notion) ·
**Alcance:** especificación campo por campo de cada tipo de evento del event log
(Contrato §3), la política de versionado aplicada (§3.1) y la política de
`event_hash` (§3.3). Precede a los tipos de Rust, igual que T1.0 precede al parser
del ledger.

> A diferencia de `docs/spec-ledger.md`, este documento **no es un mirror**: el
> Contrato del Run da la tabla evento→emisor→payload-relevante y las políticas de
> versionado/hashing, pero no el detalle campo por campo de cada payload. Ese
> detalle se deriva acá, con cita de la fuente cuando existe y marcado
> **[inferido]** cuando no hay texto normativo literal que lo respalde.

## 0. Nota sobre el conteo de eventos

El Contrato del Run dice textualmente *"Los 30 tipos de evento (24 filas; varias
agrupan variantes emparentadas)"*. Se verificó la tabla real del documento (parseo
del JSON fuente, no transcripción manual): tiene **25 filas y 31 `kind` distintos**
(20 filas de 1 kind + 4 filas de 2 kinds + 1 fila de 3 kinds = 31). El "30 (24
filas)" del texto introductorio no coincide con su propia tabla — probablemente un
conteo desactualizado tras agregarse una fila. **Reportado al usuario; decisión: este
documento especifica los 31 kinds tal como están enumerados en la tabla real**, que
es el contenido normativo (la prosa que los cuenta no lo es). Si el corpus se
corrige en Notion, este documento se actualiza en consecuencia.

## 1. Envelope común

Todo evento comparte la misma tupla persistida (Contrato §3):

| Campo | Tipo | Notas |
|---|---|---|
| `run_id` | `RunId` (ULID) | identifica el run |
| `seq` | `u64` | orden monotónico dentro del run — define el orden de replay |
| `timestamp` | `DateTime<Utc>` | reloj inyectado (`Clock` trait, nunca `SystemTime::now()` directo) |
| `node_id` | `Option<NodeId>` | ausente para eventos de alcance run (`run_created`, `run_paused`, ...) |
| `kind` | string | uno de los 31 nombres de este documento, con su sufijo `_vN` si no es la v1 |
| `payload_json` | JSON | específico de cada `kind` — detallado en §4 |
| `schema_version` | `u32` | versión *del payload de ese kind*, no global (§2) |

**Corrección post-implementación**: la primera versión de este documento repetía
`node_id` (y en algunos casos `from_node`/`author_node_id`) dentro de varios
payloads que ya corren en el contexto de un nodo — dato redundante con el `node_id`
del envelope, que podía desincronizarse del real. Se corrigió en
`NodeStartedPayload`, `ArtifactWrittenPayload`, `ContextAssembledPayload`,
`ScopeCheckedPayload` (queda solo `task_id` opcional), `NodeFinishedPayload`,
`NodeFailedPayload`, `HookExecutedPayload`, `NodeReroutedPayload` (queda solo
`to_node`), `GateWaitingPayload`, `GateResolvedPayload`, `QuestionsAnsweredPayload`,
`LoopIterationPayload`, `FindingPostedPayload`, `ChildRunCreatedPayload` y
`ChildRunFinishedPayload`: ninguno de estos payloads vuelve a declarar `node_id`
—se lee del envelope— y las tablas de campo de la sección 5 ya reflejan esto.

Los siete campos de la tupla, **en este orden**, son también los que participan en
`event_hash` (§3).

## 2. Política de versionado (§3.1 del Contrato)

Cuatro reglas, aplicadas desde el primer commit:

1. **Versión por tipo, no global.** `schema_version` versiona *ese* `kind`;
   `criteria_checked` puede estar en v3 mientras `run_created` sigue en v1.
2. **Dentro de una versión, solo cambios compatibles** (agregar campos opcionales).
   Renombrar, eliminar o cambiar el tipo de un campo existente es un **`kind`
   nuevo** (`criteria_checked_v2`), nunca una migración del log — el event log es
   append-only (I2) y una migración introduciría una operación falible sobre la
   fuente de verdad.
3. **Lector tolerante, escritor estricto.** Campos desconocidos se ignoran al leer;
   al escribir, el payload se valida contra el schema — un evento inválido es un
   bug del engine, no un warning.
4. **`kind` desconocido → run parcialmente interpretado, nunca `broken`.** Ligado a
   I11: ninguna degradación es silenciosa.

El JSON Schema se **genera desde los tipos de Rust** (fuente de verdad = código) y
se versiona en el repo; cualquier cambio de payload produce diff visible en PR. La
normalización a modelo de dominio ocurre en memoria al leer — **ningún `_vN`
aparece en `status`, `stats`, el recibo ni ninguna otra superficie de usuario**.

**Versión inicial de cada `kind` en M-0**: todos arrancan en **v1**. No hay
historial previo — es la primera implementación.

## 3. Política de `event_hash` (§3.3 del Contrato — spec, no implementación)

> Implementado (T2.5): `yunta-storage` calcula el hash en `append_event` y lo
> verifica en `Storage::verify_chain` / `yunta verify <run_id>`, siguiendo esta
> política al pie de la letra. La codificación concreta de los campos es
> length-prefixed (`len:bytes;`) para que ningún límite de campo sea ambiguo.

- **Fórmula**: `event_hash = SHA-256(prev_event_hash || campos_estructurales_en_orden_fijo)`.
- **Campos que participan**, en el orden fijo del schema (§1 de este documento, no
  orden alfabético): `run_id, seq, timestamp, node_id, kind, payload_json,
  schema_version`.
- Se calcula sobre **los bytes exactamente como se persistieron**, antes de
  cualquier normalización de lectura (§2) — la integridad de la cadena es
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
  criptográfica (A-06, deuda consciente, sin ADR de diseño todavía) es una capa
  aparte y deliberadamente separada.
- Corresponde a **I26**.

## 4. Mapeo `AgentEvent` (Spec Adapter) → `kind` del event log

El trait `Adapter` emite 6 variantes de `AgentEvent` (`SessionOpened`, `ToolUse`,
`Usage`, `Note`, `Completed`, `Failed`), pero solo dos `kind` del event log están
atribuidos al adapter en la tabla del Contrato: `agent_session_opened` y
`agent_message`. El Contrato no explicita el mapeo completo; se resuelve acá
**[inferido]**, apoyado en principios ya normativos:

| `AgentEvent` | `kind` resultante | Razón |
|---|---|---|
| `SessionOpened` | `agent_session_opened` | mapeo directo — O1 del Spec Adapter: obligatorio y siempre primero |
| `ToolUse` / `Usage` / `Note` | `agent_message` (con `message_type` distinguiendo) | es el único otro `kind` atribuido al adapter en la tabla; `agent_message` es genérico ("resumen/uso de tokens") |
| `Completed` / `Failed` | **ninguno directo** — dispara la verificación del engine, que emite `node_finished` / `node_failed` | §1 del Contrato: "la palabra del agente nunca es evidencia"; D14: no existe verifier-agente, la verificación es fase del engine, no un evento que el adapter pueda producir por sí mismo; O2 del Spec Adapter solo obliga a que el *stream* termine con uno de los dos, no que se persista tal cual |

Si esta lectura no es la intención original, es exactamente el tipo de cosa a
corregir con una nota tuya antes de que se convierta en tipos de Rust.

## 5. Los 31 tipos de evento, campo por campo

Convención de esta sección: **Fuente** cita la columna "Payload relevante" del
Contrato §3 tal cual; **Campos** expande eso a nombre/tipo/obligatoriedad/nota,
marcando `[inferido]` lo que no tiene respaldo textual directo.

---

### 5.1 `run_created` — engine
**Fuente:** manifest hash, inputs, modo, `promoted_from?`

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `manifest_hash` | string (hash) | sí | identifica el manifest congelado (§2.1) |
| `inputs` | mapa string→valor | sí | inputs resueltos y validados (§2.3), defaults incluidos |
| `mode` | string | sí | nombre del modo elegido (§10.1) |
| `promoted_from` | `Option<RunId>` | no | presente solo si este run nace de una promoción (§10.2) |
| `yunta_schema` [inferido] | string (semver-range) | no | declarado o inferido del binario (§2.1) — congelado junto al resto |
| `base_branch` / `base_commit` [inferido] | string | sí | necesarios para el worktree (§7.3) y mencionados como parte del manifest en §2.1 |

### 5.2 `runner_resolved` — engine
**Fuente:** rol, candidato elegido, candidatos descartados y causa

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `role` | string | sí | nombre del runner (`runner:`, D28) |
| `chosen` | `{adapter, model, agent?}` | sí | binding resuelto y congelado (I17) |
| `discarded` | lista de `{candidate, reason}` | sí (puede ser vacía) | candidatos no elegidos y por qué — nunca vacío sin motivo si hubo &gt;1 candidato |

### 5.3 `baseline_captured` — engine
**Fuente:** comando de suite, resultados, hash

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `command` | string | sí | `baseline.suite` resuelto de config |
| `results` [inferido] | `{exit_code, summary}` | sí | resultado crudo de correr la suite una vez al abrir el run |
| `hash` | string | sí | hash del resultado, insumo de `baseline_compare` (D18) |

### 5.4 `node_started` — engine
**Fuente:** node_id, intento N

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `attempt` | `u32` | sí | 1-indexado; sube con cada reintento (§5.2 del Contrato) |

### 5.5 `agent_session_opened` — adapter
**Fuente:** session_id, agente, modelo, capacidades

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `session_id` | `SessionId` (opaco) | sí | persiste para `resume` (O1, I17) |
| `agent` | `Option<String>` | no | agente nombrado del adapter, si se pidió (`agent:`, D29) |
| `model` | string | sí | modelo efectivamente usado |
| `capabilities` | `Capabilities` (bools: resume_session, edit_hooks, permission_profiles, custom_agents, usage_reporting, run_tools) | sí | snapshot de capacidades del adapter en ese momento (constantes tras construcción, A2) |

### 5.6 `agent_message` — adapter
**Fuente:** resumen/uso de tokens (nunca el texto completo)

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `message_type` [inferido] | enum `tool_use \| usage \| note` | sí | distingue cuál variante de `AgentEvent` originó el mensaje (§4) |
| `tool_name` [inferido] | `Option<string>` | solo si `tool_use` | de `ToolUse.name` |
| `target_digest` [inferido] | `Option<string>` | solo si `tool_use` | de `ToolUse.target_digest` — nunca contenido completo (O3) |
| `input_tokens` / `output_tokens` [inferido] | `Option<u64>` | solo si `usage` | de `Usage` |
| `cached_input_tokens` [inferido] | `Option<u64>` | no | opcional incluso dentro de `usage` — solo si el CLI distingue lectura de caché (§8.4) |
| `text` [inferido] | `Option<string>` | solo si `note` | resumen mecánico `N bytes, sha256 <prefijo>` del texto de `Note` — jamás el contenido (I12/O3): el log no debe poder portar un secreto que la nota contenía (DI-09 endureció el "acotado" original a contenido-cero con este racional) |

### 5.7 `artifact_written` — engine
**Fuente:** node_id, path, content hash

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `path` | string | sí | relativo a `run.dir/artifacts/` |
| `content_hash` | string | sí | los artifacts son inmutables (I4); esto es lo que se verifica en `resume` |

### 5.8 `context_assembled` — engine
**Fuente:** node_id, fuentes resueltas, hash por segmento de estabilidad (§9.1)

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `sources` | lista de `{source_id, kind}` | sí | qué `ContextSource` se resolvieron |
| `segment_hashes` | mapa `stable \| run-stable \| volatile` → hash | sí | orden fijo estable→run-estable→volátil→prompt (I19); insumo directo de replay/diff (T13.1/T13.2) |

### 5.9 `task_registered` — engine
**Fuente:** task_id, criteria, scope, deps

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string (patrón de T1.0 §2) | sí | — |
| `criteria` | lista de `{cmd, type?}` | sí | copia congelada del ledger (T1.0 §2.1) |
| `scope` | lista de globs | sí | — |
| `depends_on` | lista de `task_id` | no | default vacío |

### 5.10 `criteria_checked` — engine
**Fuente:** task_id, fase pre/post, exit code por criterio, ejecutado o reutilizado de caché (§5.4)

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `phase` | enum `pre \| post` | sí | pre-check en rojo vs. post-check (§5.2) |
| `results` | lista de `{cmd, exit_code, type?, reused: bool, duration_ms?}` | sí | `reused=true` cuando la memoización (§5.4, fuera de alcance de implementación en M-0 salvo lo mínimo de T5.9) sirvió el resultado sin re-ejecutar; `duration_ms` (DI-15, aditivo D70) es el costo observado de la ejecución — ausente en `reused=true` y en eventos pre-DI-15 |

### 5.11 `task_status_changed` — engine
**Fuente:** task_id, estado nuevo, evento que lo justifica

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `new_status` | enum `pending \| ready \| running \| done \| blocked \| failed` [inferido, valores exactos a confirmar contra T5.2] | sí | solo el engine emite este evento — ningún agente tiene vía para marcarlo (I5) |
| `caused_by` | referencia a `seq` de otro evento | sí | el evento (p. ej. `criteria_checked`) que justifica la transición |

### 5.12 `scope_checked` — engine
**Fuente:** task_id/node_id, diff observado, violaciones

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | `Option<string>` | no | presente si el chequeo es de una tarea dentro de un `loop`; ausente para un chequeo de nodo suelto — el nodo en ambos casos es el `node_id` del envelope |
| `diff` | lista de paths | sí | de `git diff` contra el scope declarado |
| `violations` | lista de paths | sí (vacía si limpio) | paths fuera de todo glob declarado |

### 5.13 `scope_expansion_requested` — engine
**Fuente:** task_id, paths, razón, criterio propuesto y su pre-check (§6.2)

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `paths` | lista de globs | sí | ampliación pedida |
| `reason` | string | sí | — |
| `proposed_criterion` | `Option<{cmd}>` | no | si el agente propone además un criterio nuevo |
| `proposed_criterion_precheck` [inferido] | `Option<{exit_code}>` | solo si hay `proposed_criterion` | D73: un criterio que ya pasa se rechaza automático sin consultar |

### 5.14 `scope_expansion_granted` / `scope_expansion_denied` — engine
**Fuente:** task_id, decisor (regla o persona), modo, conteo del run

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `decided_by` | enum `rule \| person` + identificador | sí | — |
| `mode` | enum `rules \| ask \| deny` | sí | modo vigente en el momento de la decisión |
| `count_this_run` | `u32` | sí | para el cap `max_per_run` (D73) |
| `denial_reason` | `Option<string>` | solo en `denied` | toda denegación produce además un `finding_posted` (D80) — no lo reemplaza, lo acompaña |

### 5.15 `node_finished` / `node_failed` — engine
**Fuente:** resultado, tokens, ¿reintentable?

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `outcome` [inferido] | dato del engine tras verificación, no el `AgentOutcome` crudo del adapter | sí | §1 del Contrato: el outcome del agente es telemetría, esto es el veredicto |
| `tokens_used` | `{input, output, cached?}` | sí | acumulado desde `Usage` (§8.4) |
| `retryable` | `bool` | solo en `node_failed` | guía la política de reintento (§5.2) |

### 5.16 `hook_executed` — engine
**Fuente:** node_id, fase before/after, comando, exit code

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `phase` | enum `before \| after` | sí | D23: hooks son ciclo del engine, no del adapter |
| `command` | string | sí | — |
| `exit_code` | `i32` | sí | — |

### 5.17 `node_rerouted` — engine
**Fuente:** nodo fallido, destino, causa, reintento N de M

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `to_node` | `NodeId` | sí | destino de `on_failure.goto` — el nodo que falló es el `node_id` del envelope |
| `cause` | string | sí | — |
| `attempt` | `u32` | sí | N de `max_reroutes` (M) — D24 |
| `max_reroutes` | `u32` | sí | — |

### 5.18 `gate_waiting` / `gate_resolved` — engine/adapter
**Fuente:** opciones, elección, quién, feedback

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `summary` | string | solo en `gate_waiting` | objeto de escalación (§5.3) |
| `evidence` | estructura del engine | solo en `gate_waiting` | nunca prosa generada por agente (I20) |
| `options` | lista de `{option, tradeoff}` | solo en `gate_waiting` | `tradeoff` es obligatorio por opción (D50) |
| `chosen_option` | `Option<string>` | solo en `gate_resolved` | — |
| `resolved_by` | `Option<string>` | solo en `gate_resolved` | usuario o identificador de quien resolvió |
| `free_text` | `Option<string>` | no | siempre disponible como canal (D50) |

### 5.19 `questions_answered` — engine
**Fuente:** node_id, hash del artifact de respuestas, canal (tty\|mcp\|pr), respondiente si se conoce

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `answers_hash` | string | sí | hash del artifact de respuestas (D86) |
| `channel` | enum `tty \| mcp \| pr` | sí | — |
| `responder` | `Option<string>` | no | si el canal lo identifica |

### 5.20 `loop_iteration` — engine
**Fuente:** iteración N, evaluación de `until`

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `iteration` | `u32` | sí | — |
| `until_result` | `bool` | sí | resultado de evaluar la condición del loop (I7: la evalúa el engine, no el agente) |

### 5.21 `finding_posted` — engine
**Fuente:** autor (nodo), hallazgo: id, severidad, título, location, detalle (§4.1)

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `finding.id` | string | sí | autor = `node_id` del envelope |
| `finding.severity` | enum | sí | usada por `findings_gate` (D85) |
| `finding.title` | string | sí | — |
| `finding.location` | string | sí | usada para deduplicación (§5.12 del Plan) |
| `finding.detail` | string | sí | — |
| `finding.proposed_criterion` | `Option<{cmd}>` | no | — |

### 5.22 `promotion_signaled` — engine
**Fuente:** razón, evidencia, modo sugerido

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `reason` | string | sí | — |
| `evidence` | estructura del engine | sí | — |
| `suggested_mode` | string | sí | debe respetar la escalera de promoción (§10.1, D44) |

### 5.23 `child_run_created` / `child_run_finished` — engine
**Fuente:** node_id, child run_id, `workflow_hash` del hijo, estado terminal

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `child_run_id` | `RunId` | sí | referencia histórica inmutable (I28, D104) — el nodo `kind: workflow` del padre es el `node_id` del envelope |
| `child_workflow_hash` | string | sí | fija qué versión del workflow hijo corrió — reproducir el padre nunca resuelve una versión nueva |
| `terminal_state` | estado | solo en `child_run_finished` | — |

### 5.24 `capability_degraded` — engine
**Fuente:** capacidad, adapter, política aplicada

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `capability` | string (nombre de campo de `Capabilities`) | sí | — |
| `adapter` | string (`id()` del adapter) | sí | — |
| `policy_applied` | string | sí | de la tabla de degradación del Spec Adapter §5 |

### 5.25 `run_paused` / `run_resumed` / `run_finished` — engine
**Fuente:** razón / estado terminal, métricas

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `reason` | string | solo en `run_paused` | presupuesto excedido, gate esperando, etc. |
| `resume_policy_applied` [inferido] | `Option<string>` | solo en `run_resumed` | qué `on_interrupt` se aplicó a cada nodo huérfano retomado |
| `terminal_state` | estado | solo en `run_finished` | — |
| `metrics` | `{cptv?, tokens, ...}` | solo en `run_finished` | derivadas del log, nunca estimadas (I20) |

## 6. Regla transversal (I12/O3)

Ningún payload de ningún `kind` de este documento puede contener: contenido completo
de archivo, prompt o respuesta del agente, ni valores de secretos. Donde el payload
necesita referenciar contenido, lleva un hash o un resumen acotado — nunca el dato
en sí. El engine redacta además cualquier valor de secreto conocido antes de
persistir, como defensa en profundidad (I12) — no como excusa para ser menos
cuidadoso en el diseño del payload.
