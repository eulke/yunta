# Spec — Payloads de eventos del event log

**Estado:** normativo v0.1 · **Alcance:** especificación campo por campo de cada
tipo de evento del event log, la política de versionado aplicada y la política de
`event_hash`. Precede a los tipos de Rust, igual que la spec del documento de tareas precede a
su parser.

> El Contrato del Run da la tabla evento→emisor→payload-relevante y las políticas de
> versionado y hashing, pero no el detalle campo por campo de cada payload. Ese
> detalle se fija acá, con cita de la fuente cuando existe y marcado
> **[inferido]** cuando no hay texto normativo literal que lo respalde.

## 0. Event count

The current Run Contract event table has 32 rows and **39 `kind` names**.
It had 31 rows and 38 kinds before `run_tool_failed` was added. The table
defines the normative set; this document specifies each payload.

## 1. Envelope común

Todo evento comparte la misma tupla persistida:

| Campo | Tipo | Notas |
|---|---|---|
| `run_id` | `RunId` (ULID) | identifica el run |
| `seq` | `u64` | orden monotónico dentro del run — define el orden de replay |
| `timestamp` | `DateTime<Utc>` | reloj inyectado (`Clock` trait, nunca `SystemTime::now()` directo) |
| `node_id` | `Option<NodeId>` | ausente para eventos de alcance run (`run_created`, `run_paused`, ...) |
| `kind` | string | One of the 39 names in this document, with a `_vN` suffix beyond v1. |
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

## 5. Los 38 tipos de evento, campo por campo

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
| `results` [inferido] | `{exit_code, summary}` | sí | resultado crudo de correr la suite una vez, en el primer despertar del run que la mide |
| `hash` | string | sí | hash del resultado, insumo de `baseline_compare` |
| `origin` | `{type: measured}` \| `{type: inherited, run}` | sí | de quién es la medición: `measured`, este run la tomó; `inherited`, nació teniéndola y `run` nombra a la raíz del linaje que la midió. Un log sin el campo se lee `measured` |

### 5.4 `node_started` — engine
**Fuente:** node_id, intento N

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `attempt` | `u32` | sí | 1-indexado; sube con cada reintento |
| `from_tree` | `TreeId` | no | el árbol del que parte este intento: contra él se mide su propio diff al cerrar |

**De qué árbol parte.** `from_tree` es el id del objeto `tree` que el árbol de
trabajo tenía cuando el intento arrancó, capturado con un índice privado para no
disputarle `.git/index` a nadie. Es lo que hace que una auditoría de `scope:` diga
qué cambió *este* nodo y no qué hay de distinto desde que nació el run: lo que un
nodo anterior dejó sin commitear es el estado del que este parte, no algo de lo
que responda. Que sea un hecho del log y no memoria del proceso es lo que lo
sostiene a través de un replay, y que sea un árbol —y no una lista de paths— es lo
que impide el reverso: un archivo que ya estaba sucio y que este intento *también*
tocó difiere del árbol de partida y sigue siendo suyo. Un evento escrito antes de
que el arranque nombrara su árbol no lo lleva, y se lee contra la base del run,
que es lo que ese log significaba (D182).

### 5.5 `agent_session_opened` — adapter
**Fuente:** session_id, agente, modelo, capacidades

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `session_id` | `SessionId` (opaco) | sí | persiste para `resume` |
| `agent` | `Option<String>` | no | agente nombrado del adapter, si se pidió (`agent:`) |
| `model` | `Option<ModelName>` | no | el modelo que el CLI reportó para la sesión; ausente cuando no reportó ninguno — nunca el pedido |
| `capabilities` | `Capabilities` (`fence`: `none \| tool_calls \| filesystem`; el resto bools: resume_session, permission_profiles, custom_agents, usage_reporting, skills, run_tools, network_isolation) | sí | snapshot de capacidades del adapter en ese momento — constantes tras construcción. Un log viejo lleva `edit_hooks` en vez de `fence`, y el lector lo lee como `none` |
| `fence` | `Coverage` (`{"coverage": "exact"}` · `{"coverage": "widened_to_roots", "roots": [...]}` · `{"coverage": "tools_only"}`) | no | cuánto del canal de escritura cercó realmente la sesión, derivado de lo que el adapter construyó; ausente cuando no construyó ninguno. El nivel viaja una vez, en `capabilities.fence` |

### 5.6 `agent_message` — adapter
**Fuente:** resumen/uso de tokens (nunca el texto completo)

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `message_type` [inferido] | enum `tool_use \| usage \| note` | sí | distingue cuál variante de `AgentEvent` originó el mensaje |
| `tool_name` [inferido] | `Option<string>` | solo si `tool_use` | de `ToolUse.name` |
| `target` [inferido] | `Option<ToolTarget>` | solo si `tool_use` | de `ToolUse.target`: `digest` siempre, `display` solo cuando el argumento nombra el repositorio — nunca contenido completo |
| `input_tokens` / `output_tokens` [inferido] | `Option<u64>` | solo si `usage` | de `Usage` |
| `cached_input_tokens` [inferido] | `Option<u64>` | no | opcional incluso dentro de `usage` — solo si el CLI distingue lectura de caché |
| `text` [inferido] | `Option<string>` | solo si `note` | resumen mecánico `N bytes, sha256 <prefijo>` del texto de `Note` — jamás el contenido: el log no debe poder portar un secreto que la nota contenía, así que el resumen es contenido-cero, no meramente acotado |

### 5.7 `artifact_written` — solo lectura
**Fuente:** node_id, path, content hash

El engine no lo escribe: todo artifact que un run adquiere entra por
`artifact_accepted` (§5.21.5). Queda como kind para que un log anterior
se lea, y un lector lo pliega como identidad de artifact: `artifact_kind`
presente da la identidad interpretada, ausente da la opaca con el nombre
bajo `artifacts/`, y el origen es `legacy` porque el evento no lo
registra.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `path` | string | sí | relativo al run dir: `artifacts/<nombre>`, con el nombre que el nodo declara en `artifacts.produces` |
| `content_hash` | string | sí | el hash de los bytes que el run escribió; sin objeto detrás, así que el `resume` cuenta el artifact como no verificable en vez de rehashearlo (Contrato §8.1) |
| `artifact_kind` | enum | no | `tasks` \| `findings` \| `questions` cuando el artifact declara `kind:`; ausente para uno opaco y para un log escrito antes del campo |

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
| `task_id` | string (mismo patrón de id que en el schema del documento de tareas) | sí | — |
| `criteria` | lista de `{cmd, type?}` | sí | copia congelada del documento de tareas |
| `scope` | lista de globs | sí | — |
| `depends_on` | lista de `task_id` | sí (puede ser vacía) | vacía cuando la tarea no depende de ninguna |

### 5.10 `criteria_checked` — engine
**Fuente:** task_id, fase pre/post, exit code por criterio, ejecutado o reutilizado de caché

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `phase` | enum `pre \| post` | sí | pre-check en rojo vs. post-check |
| `results` | lista de `{cmd, exit_code, type?, reused: bool, duration_ms?}` | sí | `reused=true` cuando la memoización (fuera de alcance de una implementación completa, salvo lo mínimo necesario) sirvió el resultado sin re-ejecutar; `duration_ms` es el costo observado de la ejecución — ausente en `reused=true` y en eventos emitidos antes de que este campo se agregara |

### 5.11 `task_status_changed` — engine
**Fuente:** task_id, estado nuevo, evento que lo justifica, commit donde aterrizó el trabajo

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `new_status` | enum `pending \| ready \| running \| done \| blocked \| failed` [inferido, valores exactos a confirmar contra la implementación del scheduler] | sí | solo el engine emite este evento — ningún agente tiene vía para marcarlo |
| `caused_by` | referencia a `seq` de otro evento | sí | el evento (p. ej. `criteria_checked`) que justifica la transición |
| `commit` | `Option<CommitSha>` | no | dónde aterrizó el trabajo de la tarea, en un `done` y en ningún otro estado: el commit que el árbol del run llevaba tras integrarlo. Es lo que vuelve a un `done` respondible desde otro run — un árbol desciende de ese commit o no tiene el trabajo |

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
| `paths` | lista de globs | solo en `scope_expansion_granted` | los paths exactos que la concesión autorizó: el scope efectivo de un intento posterior se deriva del log sin volver a aparear la concesión con el pedido que la precedió. Un log escrito antes del campo lo lee vacío |
| `denial_reason` | `Option<string>` | solo en `scope_expansion_denied` | toda denegación produce además un `finding_posted` — no lo reemplaza, lo acompaña |

### 5.15 `node_finished` / `node_failed` — engine
**Fuente:** resultado, tokens, ¿reintentable?

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `outcome` [inferido] | dato del engine tras verificación, no el `AgentOutcome` crudo del adapter | solo en `node_finished` | el outcome del agente es telemetría, esto es el veredicto |
| `outcome` / `artifacts` / `died` | frase \| lista de artifacts que no cerraron \| la sesión que murió | solo en `node_failed` | por qué falló, como dato: uno de los tres, plano sobre el payload; ver abajo |
| `tokens_used` | `{input, output, cached?}` | sí | acumulado desde `Usage` |
| `retryable` | `bool` | solo en `node_failed` | guía la política de reintento; lo fija quien gobierna el presupuesto, de modo que un intento terminal nunca se registra como reintentable |

**La falla es dato, no prosa.** La falla toma una de tres formas, planas sobre el
payload: `outcome: <frase>`, una falla que el engine enuncia en una oración,
`artifacts: [...]`, un elemento por artifact declarado que no cerró, o `died:
{adapter, exit?}`, una sesión que terminó sin evento terminal. Cada elemento
es una de cuatro: el archivo — `path` y uno de `artifact-missing`, `artifact-empty`,
`artifact-oversized` (con bytes y techo) o `artifact-unreadable` —, un documento que
nadie entregó (`artifact-undelivered`): el `node` que lo declaró y el `artifact`
—la identidad— que quedó debiendo; el contenido —el path del documento, la `kind`
que fija su forma y todos sus diagnósticos— o un artifact que ningún run tiene
(`artifact-unheld`): el `run` que lo debe, el `producer` de ese run al que se le
pidió cuando la referencia nombra uno, y el `artifact` que se le pidió. Solo el
primero nombra un path: el cierre abrió un archivo únicamente cuando el nodo lo
escribe. Un documento que entra por la herramienta de entrega y un nodo de
composición no escriben archivo, así que sus elementos no nombran ninguno. El texto que ve una persona se
produce al leer el evento, nunca al escribirlo (D133). Un payload que lleva
`outcome:` solo se lee como la falla de una frase, sin migración: es la tolerancia
de lectura de §3.1 del Contrato aplicada a este campo.

**Una sesión que muere dice cómo salió.** `died` nombra el `adapter` que la abrió
y, cuando esa sesión tenía proceso propio, su `exit`: `end`, una unión cerrada
—`{type: code, code}` o `{type: signal, signal}`, y `unknown` para un `type` que
este binario no conoce—, y `stderr_tail`, las últimas 20 líneas que el hijo
escribió, con todo valor del entorno de la sesión reemplazado por `[redacted]`.
Una sesión sin proceso propio no lleva `exit`. El engine pregunta sólo a la sesión
cuyo stream terminó sin decir nada; el adapter nunca inventa un terminal (D180).

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
| `origin` | enum `on_failure \| gate_choice` | sí | qué mecanismo reruteó; un log viejo sin el campo lo lee como `on_failure` |
| `attempt` | `Option<u32>` | solo en `on_failure` | N de `max_reroutes` (M); ausente en una elección de gate, que no es un reintento |
| `max_reroutes` | `Option<u32>` | solo en `on_failure` | — |

### 5.18 `gate_waiting` / `gate_resolved` — engine/adapter
**Fuente:** opciones, elección, quién, feedback

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `summary` | string | solo en `gate_waiting` | objeto de escalación |
| `evidence` | lista de `{label?, value}` | solo en `gate_waiting` | la adjunta el engine desde el log; nunca prosa generada por agente. Un hecho que se nombra solo (`exit 1`) no lleva `label`. Un log anterior a la estructura trae un string y se lee como el único hecho sin etiqueta que siempre fue |
| `options` | lista de `{id, label, tradeoff}` | solo en `gate_waiting` | `tradeoff` es obligatorio por opción |
| `external_ref` | `Option<string>` | solo en `gate_waiting` | la referencia propia del forge para este gate: la URL del pull request (Contrato §5.6); ausente en la escalación interna, que no sale del run |
| `chosen_option` | `Option<string>` | solo en `gate_resolved` | — |
| `resolved_by` | `Option<string>` | solo en `gate_resolved` | usuario o identificador de quien resolvió |
| `approved_sha` | `Option<CommitSha>` | solo en `gate_resolved` | el commit que cubre la aprobación del forge: contra él se compara la cabeza del pull request para decidir si la aprobación sigue en pie |
| `free_text` | `Option<string>` | solo en `gate_resolved` | siempre disponible como canal para quien resuelve |

### 5.19 `questions_asked` / `questions_answered` — engine
**Fuente:** node_id; hash e ids del documento `questions` y tokens de la sesión que
preguntó / hash del artifact de respuestas, canal (tty\|mcp), respondiente si se
conoce

Un nodo que declara `questions` cierra entero —hooks, scope, artifacts— y registra
`questions_asked` en vez de un terminal; entre ese hecho y `questions_answered` el
nodo espera, y el `node_finished` que el cierre difirió llega después de la
respuesta. Un `questions_asked` sin preguntas es irrepresentable: un nodo que no
preguntó nada termina en el mismo cierre.

`questions_asked`:

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `questions_hash` | string | sí | hash del documento `questions` que el nodo entregó |
| `questions` | `[string]` | sí | los ids que esperan respuesta; nunca vacío |
| `tokens_used` | `TokenUsage` | sí | lo que gastó la sesión que preguntó; la contabilidad del intento cierra acá |

`questions_answered`:

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

### 5.21.1 `finding_updated` — engine
**Fuente:** autor (nodo), el hallazgo entero en su estado nuevo

Un hallazgo se reemplaza, nunca se fusiona: el payload lleva el hallazgo
completo, así que un campo ausente está ausente. Solo el nodo que posteó
un id puede actualizarlo, y un id retirado no se actualiza. El estado
anterior queda en el log: lo que el run tiene es el último.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `finding` | objeto | sí | mismos campos que `finding_posted`; `finding.id` nombra el hallazgo que reemplaza |

### 5.21.2 `finding_withdrawn` — engine
**Fuente:** autor (nodo), id del hallazgo y el motivo

Retirar es definitivo: un id retirado no se postea, ni se actualiza, ni
se retira de nuevo. Un hallazgo que vuelve es un id nuevo. El log
conserva el hallazgo y el motivo por el que dejó de estar en pie.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `id` | string | sí | un hallazgo que este nodo posteó y no retiró |
| `reason` | string | sí | no vacío; por qué ya no está en pie |

### 5.21.3 `finding_refused` — engine
**Fuente:** autor (nodo), la operación que no se aceptó y por qué

Un hallazgo que el engine no toma es un hecho del run, no algo que solo
vio la sesión: la tasa a la que un run reporta mal es medible desde el
log. Un rechazo no cambia ningún hallazgo.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `operation` | enum | sí | `post` \| `update` \| `withdraw` |
| `id` | `Option<string>` | no | el id que la llamada nombró, cuando nombró uno que parsea |
| `report` | objeto | sí | el documento y cada problema, con la forma de §5.15 |

### 5.21.4 `artifact_submitted` — engine
**Fuente:** node_id, el artifact que una sesión entregó y el veredicto

Toda entrega queda registrada, aceptada o no. Una aceptación lleva el
hash de los bytes canónicos que el run guardó; un rechazo lleva el reporte
entero, de modo que qué se rechazó y por qué se deriva del log sin
reconstruir la sesión. Son dos hechos, no uno: este evento es la llamada
que la sesión hizo y cómo se le respondió, y está en el log haya
aterrizado el documento o no; una entrega aceptada es además un artifact
que el run tiene, y eso lo dice su propio `artifact_accepted` (§5.21.5)
con origen `submitted`.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `name` | string | sí | el nombre de la vista del documento entregado, `<kind>.yaml`: el nodo declara el kind, así que la entrega no nombra nada |
| `artifact_kind` | enum | sí | `tasks` \| `findings` \| `questions`; nombrado `artifact_kind` porque el envelope ya usa `kind` |
| `outcome.accepted.content_hash` | string | en aceptación | hash del YAML canónico, que es el objeto bajo `objects/` que el `artifact_accepted` de esa entrega nombra |
| `outcome.refused.report` | objeto | en rechazo | el documento y cada problema, con la forma de §5.15 |

### 5.21.5 `artifact_accepted` — engine
**Fuente:** node_id del productor, identidad del artifact, content hash y origen

Un artifact es un hecho del log, no un archivo que alguien puede haber
reemplazado: este evento dice qué artifact es, con qué bytes y cómo el
run lo obtuvo. La identidad es lo que un lector pregunta —un kind para
los documentos que el engine interpreta, un nombre para los opacos—, así
que resolver un artifact no depende de la ruta en que se escribió.
`artifacts/` es la vista que el engine escribe desde el log, nunca la
respuesta a él.

El productor es el `node_id` del envelope, ausente para lo que el run
adquiere sin nodo propio: un input `type: document`, un mount, una
promoción. Lo que el run tiene de una identidad es la última aceptación
de esa identidad.

El campo se llama `artifact`, no `kind`, por la misma razón que
`artifact_written.artifact_kind` (§5.7): el `kind` del envelope ya ocupa
ese nombre en el objeto. Los discriminantes de `artifact` y de `origin`
viven un nivel adentro, donde no colisionan con él.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `artifact.type` | enum | sí | `interpreted` \| `opaque` |
| `artifact.kind` | enum | en `interpreted` | `tasks` \| `findings` \| `questions` |
| `artifact.name` | string | en `opaque` | el nombre bajo `artifacts/`, anidado incluido |
| `content_hash` | string | sí | el hash de los bytes aceptados |
| `origin.kind` | enum | sí | `submitted` (una sesión lo entregó por su tool) \| `ingested` (un nodo de comando escribió el archivo) \| `derived` (el engine lo derivó del log) \| `answered` (respuestas a un `questions`) \| `input` \| `inherited` \| `legacy` |
| `origin.input` | string | en `input` | el input `type: document` por el que entró |
| `origin.run` | string | en `inherited` | el run del que viene: mount, salida de hijo, promoción |
| `origin.producer` | string | no | en `inherited`, el nodo que lo produjo allá; ausente si ese run tampoco lo produjo con un nodo |

`legacy` es el origen de un `artifact_written` plegado (§5.7): el run
tuvo el artifact y el log no dice cómo.

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
| `policy_applied` | string | sí | de la tabla de degradación de capacidades del adapter, o —cuando el listener MCP de `run_tools` no puede abrir, o cuando abrió y la sesión no recibió ninguna de sus tools— el texto que dice que la sesión corre sin run tools y por qué |

### 5.25 `write_refused` — adapter (por el engine)
**Fuente:** la sesión que la rechazó, y qué iba a escribir

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `session_id` | `SessionId` | sí | la sesión abierta cuando el cerco rechazó la escritura |
| `target` | `ToolTarget` | sí | el path, relativo al worktree cuando está bajo él — el mismo tipo que `agent_message.target` |

### 5.25a `run_tool_failed` — adapter (recorded by the engine)

A failed call to a known tool on the ephemeral `yunta-run` server. This is a
nonterminal session event; it does not claim the node failed because of the
call. The node ledger retains only the last failure of each attempt for status,
while the append-only log retains every occurrence.

| Field | Type | Required | Meaning |
|---|---|---|---|
| `session_id` | `SessionId` | yes | The session that made the call. |
| `tool` | known run-tool name | yes | Resolved through the shared run-tool catalog. |
| `cause` | `approval_blocked \| call_failed` | yes | A closed classification, never a CLI error message. |

The payload contains no arguments, tool response, or free-form error text.

### 5.26 `run_paused` / `run_resumed` / `run_finished` — engine
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
