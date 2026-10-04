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

The current Run Contract event table has 43 rows and **53 `kind` names**.
It had 42 rows and 51 kinds before `service_unreachable` and
`service_reachable` were added. The table
defines the normative set; this document specifies each payload.

## 1. Envelope común

Todo evento comparte la misma tupla persistida:

| Campo | Tipo | Notas |
|---|---|---|
| `run_id` | `RunId` (ULID) | identifica el run |
| `seq` | `u64` | orden monotónico dentro del run — define el orden de replay |
| `timestamp` | `DateTime<Utc>` | reloj inyectado (`Clock` trait, nunca `SystemTime::now()` directo) |
| `node_id` | `Option<NodeId>` | ausente para eventos de alcance run (`run_created`, `run_paused`, ...) |
| `kind` | string | One of the 48 names in this document, with a `_vN` suffix beyond v1. |
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

## 5. Los 48 tipos de evento, campo por campo

Convención de esta sección: **Fuente** cita la columna "Payload relevante"
tal cual está documentada; **Campos** expande eso a nombre/tipo/obligatoriedad/nota,
marcando `[inferido]` lo que no tiene respaldo textual directo.

---

### 5.1 `run_created` — engine
**Fuente:** manifest hash, inputs, modo, `promoted_from?`

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `manifest_hash` | string (hash) | sí | identifica el manifest congelado |
| `checkout` | ruta | no; omitido en un run que trabaja en el checkout de una persona | el checkout propio en que trabaja el run; todo lector toma de acá el árbol del run (D235). Un log anterior al campo no nombra ninguno, y su árbol es el que lleva el nombre del run bajo la raíz de worktrees que congeló el manifest |
| `inputs` | mapa string→valor | sí | inputs resueltos y validados, defaults incluidos |
| `mode` | string | sí | nombre del modo elegido |
| `promoted_from` | `Option<RunId>` | no | presente solo si este run nace de una promoción |
| `yunta_schema` [inferido] | string (semver-range) | no | declarado o inferido del binario — congelado junto al resto |
| `base_branch` / `base_commit` [inferido] | string | sí | necesarios para el worktree y forman parte del manifest congelado |
| `environment` | `{shell, path}` | no | con qué corren los comandos del run —criterios, nodos `bash`, hooks—: el `sh` encontrado en ese `PATH` y el `PATH` en orden; ausente en logs anteriores |
| `left_out` | lista de `{node, lacks: [ConfigKey]}` \| `{node, through}` | sí; vacía —y omitida del log— cuando no queda nada fuera | los nodos `optional: true` que el run deja fuera porque su config congelada no declara lo que necesitan (`lacks`, las mismas claves que `node_failed.unset`), y los nodos a los que solo lleva uno de ellos (`through`). Se decide una vez, al nacer el run: el replay nunca vuelve a leer una config para saber su propio grafo (D204) |
| `opens_on_base` | bool | falso —y omitido— salvo en un run nacido en un checkout propio con exactamente su `base_commit` | lo que deja medir la suite en otro checkout de ese commit mientras el run sigue (D232); un log anterior al campo lo lee falso y mide en su propio árbol |

### 5.2 `runner_resolved` — engine
**Fuente:** rol, candidato elegido, candidatos descartados y causa

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `runner` | string | sí | nombre del runner (`runner:`); el lector acepta también `role`, el nombre anterior del campo |
| `chosen` | `{adapter, model, agent?}` | sí | binding resuelto y congelado |
| `discarded` | lista de `{candidate, reason}` | sí (puede ser vacía) | candidatos no elegidos y por qué — nunca vacío sin motivo si hubo &gt;1 candidato |

Un nodo con sesión resuelve su runner antes de armar su contexto: el contexto le nombra a la sesión las run tools como las nombra el CLI de ese runner (`tool_naming`), así que `runner_resolved` precede a su `context_assembled`.

### 5.3 `baseline_captured` — engine
**Fuente:** comando de suite, resultados, hash

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `command` | string | sí | `baseline.suite` resuelto de config |
| `results` [inferido] | `{exit_code, summary}` | sí | resultado crudo de correr la suite una vez, en el primer despertar del run que la mide |
| `hash` | string | sí | hash del resultado, insumo de `baseline_compare` |
| `origin` | `{type: measured}` \| `{type: inherited, run}` | sí | de quién es la medición: `measured`, este run la tomó; `inherited`, nació teniéndola y `run` nombra a la raíz del linaje que la midió. Un log sin el campo se lee `measured` |
| `tree` | `Option<TreeId>` | no | el árbol sobre el que se midió la suite: una invocación posterior toma la respuesta para ese árbol. Ausente en una medición heredada —habla del árbol con que abrió el run que la midió— y en un log anterior al campo |
| `duration_ms` | `Option<u64>` | no | cuánto tardó la medición; ausente donde `tree` lo está |

### 5.4 `node_started` — engine
**Fuente:** node_id, intento N

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `attempt` | `u32` | sí | 1-indexado; sube con cada reintento |
| `from_tree` | `TreeId` | no | el árbol del que parte este intento: contra él se mide su propio diff al cerrar |
| `found` | sha de commit git | no; ausente cuando no se commiteó nada | el commit que el arranque hizo de lo que el árbol del run tenía sin commitear —lo que una persona editó con el run estacionado, lo que dejó un intento interrumpido—; ausente si no había nada, si el nodo trabaja en un checkout que abrió su grupo, si otro nodo trabajaba en el mismo árbol o si el run trabaja sin árbol propio. Un nodo con checkout propio también lo commitea: su checkout se abrió sobre eso, y la rama donde aterriza su trabajo tiene que tenerlo (D201) |
| `run_scope` | lista de globs exactos | no; solo en un nodo con `scope: run` | lo que el run había cambiado desde su base cuando arrancó este intento —el diff entre `base_commit` y `from_tree`—: los paths que el intento puede cambiar, fijados acá para que su cierre audite contra lo que se le dio y no contra lo que el run cambie después (D205) |

**De qué árbol parte.** `from_tree` es el id del objeto `tree` que el árbol de
trabajo tenía cuando el intento arrancó, capturado con un índice privado para no
disputarle `.git/index` a nadie. Es lo que hace que una auditoría de `scope:` diga
qué cambió *este* nodo y no qué hay de distinto desde que nació el run: lo que un
nodo anterior dejó en el árbol es el estado del que este parte, no algo de lo
que responda. Que sea un hecho del log y no memoria del proceso es lo que lo
sostiene a través de un replay, y que sea un árbol —y no una lista de paths— es lo
que impide el reverso: un archivo que ya estaba sucio y que este intento *también*
tocó difiere del árbol de partida y sigue siendo suyo. Un evento escrito antes de
que el arranque nombrara su árbol no lo lleva, y se lee contra la base del run,
que es lo que ese log significaba (D182).

**Lo que encontró.** Un nodo que trabaja en el árbol del run parte de una rama
que tiene todo lo que el árbol tiene: lo que nadie commiteó se commitea antes del
arranque, con el mensaje `found in the run's tree before node <id> started`, y
—si el intento anterior de ese nodo nunca cerró— un cuerpo que dice que es lo que
ese intento dejó. El commit, la captura y el `node_started` son un solo paso, así
que `from_tree` es exactamente el árbol de `found` (D201).

### 5.5 `agent_session_opened` — adapter
**Fuente:** session_id, agente, modelo, capacidades

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `session_id` | `SessionId` (opaco) | sí | persiste para `resume` |
| `agent` | `Option<String>` | no | agente nombrado del adapter, si se pidió (`agent:`) |
| `model` | `Option<ModelName>` | no | el modelo que el CLI reportó para la sesión; ausente cuando no reportó ninguno — nunca el pedido |
| `capabilities` | `Capabilities` (`fence`: `none \| tool_calls \| filesystem`; `tool_naming`: `bare \| mcp_prefixed \| mcp_prefixed_underscored`; el resto bools: resume_session, permission_profiles, custom_agents, usage_reporting, skills, run_tools, network_isolation) | sí | snapshot de capacidades del adapter en ese momento — constantes tras construcción. `tool_naming` es cómo el CLI le nombra al modelo las tools de un servidor MCP (`yunta_task`, `mcp__yunta-run__yunta_task`, `mcp__yunta_run__yunta_task`): el nombre con el que todo texto del engine le menciona una run tool a esa sesión. Un log viejo lleva `edit_hooks` en vez de `fence`, y el lector lo lee como `none`; uno sin `tool_naming`, como `bare` |
| `fence` | `Coverage` (`{"coverage": "exact"}` · `{"coverage": "widened_to_roots", "roots": [...]}` · `{"coverage": "tools_only"}`) | no | cuánto del canal de escritura cercó realmente la sesión, derivado de lo que el adapter construyó; ausente cuando no construyó ninguno. El nivel viaja una vez, en `capabilities.fence` |
| `task_id` | `Option<TaskId>` | no | la tarea que trabaja una sesión de `loop`; ausente para la sesión propia de un nodo. Con `concurrency` > 1 las sesiones de un loop se intercalan, y es lo que dice de qué tarea es cada una |
| `continues` | `Option<SessionId>` | no | la sesión que esta reanuda: la misma conversación, retomada después de la respuesta a lo que pidió (una ampliación de scope concedida o denegada). Ausente para una sesión que abrió nueva; cuando la reanudación no fue posible, un `capability_degraded` (`resume_session` → sesión nueva) lo dice |

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
| `sources` | lista de `{source_id, kind, content_hash, absent?}` | sí | qué `ContextSource` se resolvieron; `absent` lists the optional `files:` paths the source did not find (D186), omitted when empty |
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
**Fuente:** task_id, fase pre/post, exit code por criterio, ejecutado o reutilizado de caché, lo que imprimió

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `phase` | enum `pre \| post` | sí | pre-check en rojo vs. post-check |
| `results` | lista de `{cmd, exit_code, type?, reused: bool, duration_ms?, output?, tail?, tree?, head?}` | sí | `reused=true` cuando la memoización (fuera de alcance de una implementación completa, salvo lo mínimo necesario) sirvió el resultado sin re-ejecutar; `duration_ms` es el costo observado de la ejecución — ausente en `reused=true` y en eventos emitidos antes de que este campo se agregara; `output` es el hash del objeto con lo que imprimió el comando, stdout y después stderr, redactado — en `reused=true`, lo que imprimió la ejecución que dio esa respuesta roja sobre el mismo árbol, y ausente si la respuesta reutilizada pasó; `tail` son sus últimas 20 líneas cuando `exit_code` no es 0, y se omite cuando pasó. Un log anterior a estos dos campos los lee ausentes. `tree` es el árbol git de lo que el checkout tenía cuando el comando respondió —para lo que se guarda la respuesta—, y `head` el commit en que estaba, solo para un comando que corre `git`; ambos ausentes en un check que el engine enuncia sin correr, y en un log anterior a ellos |
| `waiting` | lista de strings (comandos) | vacía —y omitida— cuando ninguna guarda esperó | los `guard` que el check no corrió porque un criterio propio de la tarea estaba en rojo; solo en `post`. Un log anterior al campo lo lee vacío |

### 5.11 `task_status_changed` — engine
**Fuente:** task_id, estado nuevo, evento que lo justifica, commit donde aterrizó el trabajo

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | — |
| `new_status` | enum `pending \| ready \| running \| done \| blocked \| failed` [inferido, valores exactos a confirmar contra la implementación del scheduler] | sí | solo el engine emite este evento — ningún agente tiene vía para marcarlo |
| `caused_by` | referencia a `seq` de otro evento | sí | el evento (p. ej. `criteria_checked`) que justifica la transición |
| `commit` | `Option<CommitSha>` | no | dónde aterrizó el trabajo de la tarea, en un `done` y en ningún otro estado: el commit que el árbol del run llevaba tras integrarlo. Es lo que vuelve a un `done` respondible desde otro run — un árbol desciende de ese commit o no tiene el trabajo |
| `left_work` | `Option<CommitSha>` | no | el commit con el trabajo que dejó el último intento de la tarea, commiteado en la rama de su unidad: en un `blocked`, trabajo desde el que una persona puede elegir continuar (`continue-work`); en el `pending` de esa reapertura, el trabajo desde el que continúa. Ausente cuando el intento no cambió nada, y en cualquier otra transición |
| `resumes` | `Option<SessionId>` | no | en el `pending` que reabre una tarea después de la respuesta al scope que pidió su sesión: esa sesión, que el ciclo siguiente reanuda en la unidad que tiene su trabajo. Ausente en cualquier otra transición |

### 5.11a `task_check_started` / `task_check_answered` — engine
**Source:** a task session's `yunta_check_task` call and the judgement it answered

| Field | Type | Required | Notes |
|---|---|---|---|
| `task_id` | string | yes, in both | the task of the session that asked |
| `closes` | `bool` | only in `task_check_answered` | whether the task would have been done had the session ended then |
| `results` | same list as `criteria_checked.results` | only in `task_check_answered` | one per criterion that ran, the run's suite guard included; `reused=true` where the invocation's cache answered it |
| `waiting` | list of strings (commands) | only in `task_check_answered`; empty — and omitted — when every guard ran | the guards the check did not run because one of the task's own criteria was red |
| `outside_scope` | list of paths | only in `task_check_answered`; empty — and omitted — when there are none | what the work changed outside the task's scope |
| `denied` | list of paths | only in `task_check_answered`; empty — and omitted — when there are none | what it changed that the project denies to every run |
| `duration_ms` | `u64` | only in `task_check_answered` | how long the whole judgement took, cache hits included |

Both are audit: a check judges work in progress, and only the attempt's close
moves the task. The question is written before the judgement runs, so a
`task_check_started` with no answer after it is a check whose session ended
first, or whose answer never reached it.

### 5.11b `deviation_declared` / `deviation_resolved` — engine
**Source:** a task session's `yunta_declare_deviation` call, and a person's answer to it

| Field | Type | Required | Notes |
|---|---|---|---|
| `task_id` | string | yes, in both | the task whose session departs |
| `from` | `{shape}` \| `{decision}` \| `{change}` \| `{criterion}` \| `outcome` | only in `deviation_declared` | what of the plan the work departs from: a shape or decision by name, one of the task's changes by where it is, one of its criteria by its command, or its outcome |
| `planned` | string | only in `deviation_declared` | what the plan says |
| `instead` | string | only in `deviation_declared` | what the work does or needs instead |
| `why` | string | only in `deviation_declared` | — |
| `accepted` | `bool` | only in `deviation_resolved` | true: the task closes as it stands if its criteria pass, less a criterion of its plan it departed from; false: its session picks the work back up with what the person said |
| `said` | string | no, in `deviation_resolved` | what the person said |
| `respecified_by` | `NodeId` | no, in `deviation_resolved` | accepted from a test the run's spec gave the task: the node that writes the task's tests again before the task goes on |

The session's own words are kept here in full: unlike its messages, a
departure is something it states to the run on purpose. Neither is audit:
`deviation_declared` adds to what the task owes a person, and a task that owes
one does not close, whatever its criteria say — not even after a scope request
its session made in the same attempt is answered. `deviation_resolved` settles
everything the task owed when it was asked: accepted, those departures become
part of what the plan is shown with from then on — and a criterion of the plan
one departs from, which nothing else supplies, stops holding the task; sent back,
they are gone, and the task's next cycle reads the answer.

### 5.12 `scope_checked` — engine
**Fuente:** task_id/node_id, diff observado, violaciones

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | `Option<string>` | no | presente si el chequeo es de una tarea dentro de un `loop`; ausente para un chequeo de nodo suelto — el nodo en ambos casos es el `node_id` del envelope |
| `diff` | lista de paths | sí | de `git diff` contra el scope declarado |
| `violations` | lista de paths | sí (vacía si limpio) | paths que el trabajo no podía cambiar: fuera de todo glob declarado, o negados a todo run |
| `denied` | lista de paths | sí; vacía —y omitida del log— si no hay ninguno | los de `violations` que el proyecto niega a todo run (`permissions.paths.deny`), cualquiera sea el scope: ningún grant los ensancha (D206) |

### 5.13 `scope_expansion_requested` — engine
**Fuente:** task_id, paths, razón, criterio propuesto y su pre-check

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | `Option<string>` | no | la tarea que pide; ausente cuando pide la sesión de un nodo, y el pedido es de ese nodo — el que figura en el evento |
| `paths` | lista de globs | sí | ampliación pedida |
| `reason` | string | sí | — |
| `proposed_criterion` | `Option<{cmd}>` | no | si el agente propone además un criterio nuevo |
| `proposed_criterion_precheck` [inferido] | `Option<{exit_code}>` | solo si hay `proposed_criterion` | un criterio que ya pasa se rechaza automático sin consultar |

### 5.14 `scope_expansion_granted` / `scope_expansion_denied` — engine
**Fuente:** task_id, decisor (regla o persona), modo, conteo del run

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí, en `scope_expansion_denied` | — |
| `task_id` | `Option<string>` | no, en `scope_expansion_granted` | la tarea que la concesión amplía; ausente en la concesión al scope de un nodo — el que figura en el evento — que una persona hace desde el menú de su falla |
| `decided_by` | enum `rule \| person \| evidence` + identificador | sí | `person` lleva quién; `evidence`, el criterio rojo de la tarea cuya salida ubicó cada path (`ruta:línea`) — un hecho que el engine verificó, no una decisión (D240) |
| `mode` | enum `rules \| ask \| deny` | sí | modo vigente en el momento de la decisión |
| `count_this_run` | `u32` | sí | para el cap `max_per_run` |
| `paths` | lista de globs | solo en `scope_expansion_granted` | los paths exactos que la concesión autorizó: el scope efectivo de un intento posterior se deriva del log sin volver a aparear la concesión con el pedido que la precedió. Un log escrito antes del campo lo lee vacío |
| `denial_reason` | `Option<string>` | solo en `scope_expansion_denied` | toda denegación produce además un `finding_posted` — no lo reemplaza, lo acompaña |

### 5.14a `scope_derived` — engine
**Fuente:** task_id, los archivos que nombran una shape suya fuera de su scope, las shapes, las demasiado comunes y el commit

Lo que una tarea puede escribir además de su scope porque ahí se nombra una shape
que es suya: el engine lo lee del árbol del run al registrar el documento de
tareas —al nacer el run o cuando un nodo lo produce— y nadie lo pide ni lo decide
(D239). No es una concesión: no cuenta para `max_per_run` ni toca el registro de
la tarea.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | string | sí | la dueña de las shapes |
| `paths` | lista de globs | sí | cada archivo exacto fuera del scope declarado que nombra, como palabra entera, una shape de la tarea con nombre de identificador, entre los del tipo de archivo de la shape. Reemplaza lo que dijo una derivación anterior |
| `shapes` | lista de strings | sí | las shapes que nombran esos archivos |
| `common` | lista de strings | sí; vacía —y omitida del log— cuando no hay ninguna | las shapes que nombran más de 20 archivos: no alcanzan ninguno |
| `at` | sha de commit git | sí | el commit en que se leyeron los archivos |

### 5.15 `node_finished` / `node_failed` — engine
**Fuente:** resultado, tokens, ¿reintentable?

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `outcome` [inferido] | dato del engine tras verificación, no el `AgentOutcome` crudo del adapter | solo en `node_finished` | el outcome del agente es telemetría, esto es el veredicto |
| `outcome` / `artifacts` / `died` / `exited` / `outside_scope` / `requested_scope` / `owed` / `unset` / `unchanged` / `denied_paths` | frase \| lista de artifacts que no cerraron \| la sesión que murió \| `{code, tail, output, origin?}` el comando que salió distinto de cero \| paths que el diff escribió fuera del `scope:` \| `{paths, reason}` que la sesión del nodo pidió \| `[{task_id, paths, reason}]` los pedidos de tareas de un loop que se le deben a una persona \| `{key, ...}` la clave de config que el nodo necesita y la config del run no declara \| `{since, failure}` el intento que corrió sobre el mismo árbol y con qué falló \| paths que el trabajo escribió y el proyecto niega a todo run | solo en `node_failed` | por qué falló, como dato: uno de los diez, plano sobre el payload; ver abajo |
| `tokens_used` | `{input, output, cached?}` | sí | acumulado desde `Usage` |
| `commit` | sha de commit git | no; en `node_finished` y `node_failed`, y ausente cuando el cierre no commiteó nada | el commit que el cierre hizo de lo que el nodo dejó en el árbol del run: ausente si nada cambió, si el nodo aterrizó desde un checkout propio, si otro nodo seguía trabajando en el mismo árbol o si el run trabaja sin árbol propio (D201) |
| `tree` | id de árbol git | no; en `node_finished` y `node_failed`, y ausente en un log escrito antes del campo | el árbol del run tal como el nodo lo dejó, después de lo que aterrizó ahí —un nodo con checkout propio nombra el árbol del run en que aterrizó, no su checkout—; un gate nombra el que vio quien decidió. Es contra lo que se mide si el pase de un invariante sigue hablando del árbol del run (Contrato §11.3) |
| `refused` | lista de paths | sí; solo en `node_failed`, vacía —y omitida del log— si no hubo ninguno | lo que el intento dejó en el árbol del run y el proyecto niega a todo run (`permissions.paths.deny`): no se commiteó y, en un run con worktree propio, volvió a como la rama lo tenía. La falla propia del nodo sigue siendo su causa (D206) |
| `retryable` | `bool` | solo en `node_failed` | guía la política de reintento; lo fija quien gobierna el presupuesto, de modo que un intento terminal nunca se registra como reintentable |

**La falla es dato, no prosa.** La falla toma una de nueve formas, planas sobre el
payload: `outcome: <frase>`, una falla que el engine enuncia en una oración,
`artifacts: [...]`, un elemento por artifact declarado que no cerró, `died:
{adapter, exit?}`, una sesión que terminó sin evento terminal, `exited: {code,
tail, output, origin?}`, un comando que el nodo corrió —su `run:`, su executor o
uno de sus hooks, que `origin` nombra cuando no es el `run:` propio— y salió
distinto de cero, con las últimas líneas que imprimió —stdout y después stderr,
redactadas como todo el log— y el objeto que guarda la salida entera, `outside_scope:
[...]`, cada path que el diff del nodo alcanzó fuera de su `scope:`, o
`requested_scope: {paths, reason}`, la ampliación que pidió la sesión del nodo,
`owed: [{task_id, paths, reason}]`, las ampliaciones que pidieron tareas de un loop
y que nadie estuvo para decidir —el loop siguió con lo que no dependía de ellas y
termina debiéndolas; su menú ofrece `grant`, que las concede, y cualquier otra
respuesta las deniega con su finding (D238)—, o
`unset: {key, ...}` —`baseline_suite`, `coverage`, `executor` (con `executor`),
`runner` o `command` (con `command`, el nombre del comando del proyecto)—, la clave de config sin la que el nodo no corre y que la config congelada
del run no declara: ningún intento de ese run puede terminar distinto, así que su
menú no ofrece `retry`, o `unchanged: {since, failure}`, un `check` que juzga el
árbol (`baseline_compare`, `coverage_gate`) al que una persona pidió correr de nuevo
sobre el mismo árbol en que falló su intento `since`: no se corre, porque el mismo
comando sobre el mismo árbol responde lo mismo, y `failure` es la falla de ese
intento —una negativa sobre otra negativa sigue nombrando el intento que corrió—. Su
menú sigue ofreciendo `retry`: la persona puede cambiar el árbol mientras decide
(D197), o `denied_paths: [...]`, cada path que el trabajo escribió y el proyecto niega
a todo run (`permissions.paths.deny`): su menú nunca ofrece `grant`, porque ningún
grant ensancha lo negado (D206). Cada elemento de `artifacts`
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
**Fuente:** node_id, fase before/after, comando, exit code, lo que imprimió

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `phase` | enum `before \| after` | sí | hooks son ciclo del engine, no del adapter |
| `command` | string | sí | — |
| `exit_code` | `i32` | sí | — |
| `output` | `ContentHash` | no | el objeto con lo que imprimió el hook, redactado; ausente en un hook que no llegó a correr (`exit_code: -1`) y en logs anteriores al campo |
| `tail` | lista de string | sí (vacía si pasó) | sus últimas 20 líneas cuando `exit_code` no es 0; vacía, y omitida en el log, cuando pasó o en logs anteriores al campo |

### 5.17 `node_rerouted` — engine
**Fuente:** nodo fallido, destino, causa, reintento N de M

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `to_node` | `NodeId` | sí | destino — el nodo que reruteó es el `node_id` del envelope |
| `cause` | string | sí | — |
| `origin` | enum `on_failure \| gate_choice` | sí | qué mecanismo reruteó; un log viejo sin el campo lo lee como `on_failure` |
| `attempt` | `Option<u32>` | solo en `on_failure` | N de `max_reroutes` (M); ausente en una elección de gate, que no es un reintento |
| `max_reroutes` | `Option<u32>` | solo en `on_failure` | — |

### 5.17a `pull_request_opened` — engine
**Fuente:** url y número del pull request, rama del run, rama base

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `url` | string | sí | dónde lo lee una persona |
| `number` | `u64` | sí | el número que el forge le dio |
| `head` | string | sí | la rama del run que el nodo empujó |
| `base` | string | sí | la rama a la que va: `project.base_branch`, o la rama de la que partió el run |

Se escribe apenas el forge responde, antes del cierre del nodo: un pull request
es un efecto afuera que existe aunque el nodo después falle. Un nodo que corre de
nuevo encuentra por la marca del run el pull request que ya abrió y registra el
mismo número (D207).

### 5.18 `gate_waiting` / `gate_resolved` — engine/adapter
**Fuente:** opciones, elección, quién, feedback

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `summary` | string | solo en `gate_waiting` | objeto de escalación |
| `evidence` | lista de `{label?, value}` | solo en `gate_waiting` | la adjunta el engine desde el log; nunca prosa generada por agente. Un hecho que se nombra solo (`exit 1`) no lleva `label`. Un log anterior a la estructura trae un string y se lee como el único hecho sin etiqueta que siempre fue |
| `options` | lista de `{id, label, tradeoff, asks?}` | solo en `gate_waiting` | `tradeoff` es obligatorio por opción; `asks` es la pregunta que una opción hace antes de contar como respuesta —la de un gate que devuelve el run a un nodo con sesión, `what should change?`— y una respuesta sin `free_text` a esa opción se rechaza |
| `external_ref` | `Option<string>` | solo en `gate_waiting` | la referencia propia del forge para este gate: la URL del pull request (Contrato §5.6); ausente en la escalación interna, que no sale del run |
| `shows` | lista de `{producer?, artifact, content_hash}` | solo en `gate_waiting` (puede ser vacía) | los artifacts que el gate puso delante de quien decide, con el hash exacto que vio: la decisión queda atada a esos bytes. Los hallazgos del run (`findings` sin `producer`) son una vista que el log deriva —cada hallazgo en pie con su nodo y sus respuestas—, guardada en `objects/` por ese hash. Omitida si no muestra nada, y en logs anteriores al campo |
| `withheld` | lista de `{option, because}` | solo en `gate_waiting` (puede ser vacía) | las opciones que el gate no ofrece y por qué: con un plan que no se puede probar como está escrito, las que siguen el run más allá del gate (D227). Cada razón es también un hecho de `evidence` con la etiqueta `withheld`. Una respuesta que elige una opción retenida se rechaza en toda superficie. Omitida si no retiene nada, y en logs anteriores al campo |
| `chosen_option` | `Option<string>` | solo en `gate_resolved` | — |
| `resolved_by` | `Option<string>` | solo en `gate_resolved` | usuario o identificador de quien resolvió |
| `approved_sha` | `Option<CommitSha>` | solo en `gate_resolved` | el commit que cubre la aprobación del forge: contra él se compara la cabeza del pull request para decidir si la aprobación sigue en pie |
| `free_text` | `Option<string>` | solo en `gate_resolved` | siempre disponible como canal para quien resuelve |

### 5.19 `questions_asked` / `questions_answered` — engine
**Fuente:** node_id; hash e ids del documento `questions` y tokens de la sesión que
preguntó / hash del artifact de respuestas, canal (tty\|mcp\|assumed), respondiente
si se conoce

Un nodo que declara `questions` cierra entero —hooks, scope, artifacts— y registra
`questions_asked` en vez de un terminal; entre ese hecho y `questions_answered` el
nodo espera, y el `node_finished` que el cierre difirió llega después de la
respuesta. Un `questions_asked` sin preguntas es irrepresentable: un nodo que no
preguntó nada termina en el mismo cierre. Una ronda sin preguntas `required` no
espera: al `questions_asked` le siguen las respuestas vacías del engine y un
`questions_answered` con canal `assumed` —cada pregunta dice en `assumes` qué toma
el trabajo por respuesta— y el nodo termina en el mismo cierre (D241).

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
| `channel` | enum `tty \| mcp \| assumed` | sí | `assumed`: nadie respondió, porque ninguna pregunta era `required`; el engine cerró la ronda con lo que cada una asume |
| `responder` | `Option<string>` | no | si el canal lo identifica |

### 5.19a `asking_opened` — engine
**Fuente:** node_id; la tarea cuyo trabajo espera la respuesta, si una pregunta

Una persona en la terminal del engine empieza a ser preguntada: la elección de un
gate, una escalación, una ronda de preguntas. La respuesta llega como el evento que
resuelve lo preguntado; este dice cuándo empezó la pregunta, que solo sabe el
engine que la hizo. Se registra solo cuando hay una persona a quien preguntar: una
superficie sin nadie estaciona el run y no abre ninguna espera. Auditoría: ningún
ledger se mueve.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `task_id` | `Option<string>` | no | la tarea que espera, cuando la pregunta es por una tarea; ausente cuando pregunta el nodo |

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

### 5.21.3a `finding_answered` — engine
**Fuente:** el nodo que responde (envelope), el hallazgo que responde y la respuesta

Un nodo responde un hallazgo que reportó otro: su trabajo lo arregló
(`fixed`) o declina hacerlo (`declined`), siempre con un porqué. La
respuesta queda al lado del hallazgo, nunca adentro: el hallazgo sigue
siendo lo que se encontró, y solo el nodo que lo reportó lo actualiza o
lo retira. Una respuesta vale mientras el hallazgo esté en pie —una
actualización o un retiro la descartan—, y la del mismo nodo reemplaza a
la anterior. No cambia ningún conteo: es lo que dice un nodo.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `node` | `NodeId` | sí | el nodo que reportó el hallazgo |
| `id` | string | sí | el id del hallazgo, como lo reportó ese nodo |
| `answer` | enum | sí | `fixed` \| `declined` |
| `why` | string | sí | no vacío |

### 5.21.3b `finding_proved` — engine
**Fuente:** el nodo que respondió `fixed` (envelope), el hallazgo y el resultado de su criterio

Una respuesta es la palabra de un nodo; lo que resuelve un hallazgo es
evidencia. Cuando cierra un nodo que respondió `fixed` un hallazgo que
propone un criterio —rechazado si ya pasaba al reportarse—, el engine lo
corre sobre el árbol que deja ese nodo y registra el resultado. Si pasa, el
hallazgo queda resuelto: sigue en pie, y deja de contar para el resultado
del run, `findings_gate` y la herencia. Si falla, el resultado queda igual,
para que una persona lea que la evidencia no respaldó la respuesta.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `node` | `NodeId` | sí | el nodo que reportó el hallazgo |
| `id` | string | sí | el id del hallazgo, como lo reportó ese nodo |
| `result` | objeto | sí | el resultado del criterio, con la forma de `criteria_checked.results` (§5.10) |

### 5.21.3c `finding_settled` — engine
**Fuente:** el gate (envelope), el hallazgo que mostró y la decisión que lo resolvió

Una persona que sigue adelante en un gate que le mostró los hallazgos del
run —eligiendo una opción que ni aborta ni re-rutea— resuelve cada hallazgo
que esa vista nombraba y que sigue en pie sin resolver: exactamente lo que se
le mostró, leído de nuevo por su hash, también cuando la decisión la registró
otro proceso mientras el run estaba estacionado. Un hallazgo resuelto sigue
en pie y deja de contar para el resultado del run, `findings_gate` y la
herencia.

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `node` | `Option<NodeId>` | no | el nodo que reportó el hallazgo; ausente para uno del engine |
| `id` | string | sí | el id del hallazgo |
| `caused_by` | `Seq` | sí | el `gate_resolved` de la decisión que lo resolvió |

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
| `probes` | lista de `{task_id, results}` | vacía —y omitida— cuando la entrega no corrió nada | lo que el engine corrió para probar el documento, tarea por tarea: cada criterio con la forma de `criteria_checked.results` (§5.10), su árbol incluido, de modo que una invocación posterior toma esas respuestas para esos árboles |

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

### 5.25a.1 `run_tool_refused` — engine

Una llamada que este binario rechazó, registrada por el servidor que la rechazó:
el log dice por qué falló una llamada sin la prosa con que se le respondió a la
sesión (D238).

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `tool` | nombre de tool de `yunta-run` conocido | sí | el tool que nombró la llamada |
| `reason` | `request_pending \| invalid_arguments \| not_offered_here \| finding_not_answerable \| refused \| engine_failed` | sí | clasificación cerrada del rechazo del propio engine |

### 5.25a.2 `service_unreachable` / `service_reachable` — engine
**Fuente:** la sesión que perdió su servicio y lo que dijo su CLI / cuánto esperó

Una sesión cuyo CLI no alcanzó el servicio que la atiende no falló por su trabajo:
el engine registra `service_unreachable`, espera —con cancelación y un tope de
tiempo despierto— a que el servicio responda y el host se asiente, registra
`service_reachable` y retoma la misma sesión. Pasado el tope no hay
`service_reachable`, y la sesión falla como cualquier otra (D242).

`service_unreachable`:

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `session_id` | `SessionId` | sí | la sesión que perdió su servicio, la que se retoma |
| `message` | string | sí | lo que dijo su CLI, redactado como todo el log |

`service_reachable`:

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `waited_ms` | `u64` | sí | lo que esperó, en tiempo despierto del host |

### 5.25b `host_suspended` — engine

The host the run works on was suspended — the machine slept — while the run was
open. The engine notices it by comparing how far the wall clock moved with how far
the time the host was awake moved between two readings: the process's monotonic
clock does not advance while the host sleeps, so a wall clock that outran it by
10 s or more says the host slept for the difference. No operating-system API is
involved. The event's timestamp is when the engine noticed, at or just after the
host woke, so the suspension spans the `slept_ms` before it. It is a fact about the
machine, not something the run did: it does not wake the run, and every duration
the run reports leaves the span out (D199).

| Field | Type | Required | Meaning |
|---|---|---|---|
| `slept_ms` | `u64` | yes | How long the host slept, in milliseconds. |

### 5.26 `run_paused` / `run_resumed` / `run_finished` — engine
**Fuente:** razón / estado terminal, métricas

| Campo | Tipo | Oblig. | Notas |
|---|---|---|---|
| `reason` | string | solo en `run_paused` | presupuesto excedido, gate esperando, etc. |
| `resume_policy_applied` [inferido] | `Option<string>` | solo en `run_resumed` | el único `on_interrupt` que todos los huérfanos resolvieron; ausente sin huérfanos o con políticas distintas |
| `policies` | lista de `{node, on_interrupt}` | solo en `run_resumed` (puede ser vacía) | cada nodo que el log dejó `running` sin evento terminal y la política a la que resolvió: la propia o el default de la config |
| `environment` | `{shell, path}` | no, solo en `run_resumed` | con qué corren los comandos desde este wake; si difiere del de `run_created`, `yunta status` lo dice |
| `checkout` | ruta | no, solo en `run_resumed` | el checkout en que trabaja el run desde este wake, cuando no es el último que nombró su log (D235) |
| `terminal_state` | estado | solo en `run_finished` | — |
| `metrics` | `{cptv?, tokens, ...}` | solo en `run_finished` | derivadas del log, nunca estimadas |
| `closed_by` | responder | no, solo en `run_finished` | quién cerró un run detenido que nadie iba a continuar (`yunta close`), con `terminal_state: cancelled`; ausente cuando el engine cerró el run llevándolo a su fin |

## 6. Regla transversal

Ningún payload de ningún `kind` de este documento puede contener: contenido completo
de archivo, prompt o respuesta del agente, ni valores de secretos. Donde el payload
necesita referenciar contenido, lleva un hash o un resumen acotado — nunca el dato
en sí. El engine redacta además cualquier valor de secreto conocido antes de
persistir, como defensa en profundidad — no como excusa para ser menos
cuidadoso en el diseño del payload.
