# Contrato del Run

**Estado:** normativo v0.5 · **Alcance:** define qué es un run, qué estado persiste, cómo se reanuda, cómo se verifica el trabajo y cómo se inyecta contexto. Todo componente de Yunta (engine, adapters, skills, executors, MCP) se escribe contra este contrato, no contra implementaciones.
# 1. Principios rectores
**El estado vive fuera del agente.** Un run persiste en dos lugares y solo dos: el **event log** (base de datos, fuente de verdad del control) y el **run.dir** (disco, fuente de verdad del contenido). La ventana de contexto de cualquier sesión de agente es efímera y descartable por diseño: cualquier sesión debe poder morir en cualquier momento y el run debe poder continuar desde otra que se rehidrate leyendo estos dos lugares. Si una pieza de información importa para el futuro del run y no está en ninguno de los dos, es un bug del nodo que la produjo.
**La palabra del agente nunca es evidencia.** Los agentes aportan inteligencia; los veredictos sobre su trabajo — completitud, criterios, scope, coverage, regresiones — los emite el engine ejecutando comandos y comparando datos propios. Un sistema donde el ejecutor certifica su propia obra no es determinista por más reglas que se le escriban; Yunta elimina la posibilidad estructural, no la conducta.
De estos dos principios se derivan todas las capacidades del contrato: resumibilidad, horizonte largo, verificación e inyección de contexto son la misma propiedad — el estado es externo, auditable y derivable — vista desde distintos ángulos.
# 2. Anatomía de un run
Un run es la ejecución de un workflow con inputs concretos, identificado por un `run_id` (ULID — ordenable por tiempo, único sin coordinación). Materialización:\`javascript
~/.yunta/runs/<run_id>/          # run.dir — \{\{run.dir\}\} en templates
├── manifest.yaml                # inmutable tras la creación (§2.1)
├── [progress.md](http://progress.md)                  # generado por el engine tras cada nodo (§8.2)
├── context/                     # fuentes resueltas por el engine (§9)
├── artifacts/                   # outputs declarados de los nodos (§4)
├── baseline/                    # snapshot de suite al abrir el run (§7)
└── scratch/                     # espacio libre de los agentes; sin garantías
~/.yunta/worktrees/<run_id>/     # \{\{run.worktree\}\} — el código
```
Las rutas mostradas son los **defaults**; su ubicación es configurable (§2.2) y queda congelada en el manifest de cada run.
El worktree vive en un árbol paralelo, nunca dentro del run.dir: las operaciones de git de un agente (clean, reset) no pueden alcanzar el estado del run.
## 2.1 Manifest
`manifest.yaml` congela todo lo necesario para interpretar el run: el workflow resuelto (las tres capas de config ya mergeadas), los inputs, el modo (§10), las versiones (yunta, hash del workflow, hash de config, **`yunta_schema`** resuelto) y la rama y commit base. `yunta_schema` es un campo opcional de cabecera del workflow (`yunta_schema: ">=1 <2"`, sintaxis semver-range); si el workflow no lo declara, se infiere la versión de schema del binario que crea el run. Es la misma política de compatibilidad N/N-1 de RFC-0004 §4.4, aplicada por workflow en vez de por pack. El engine nunca relee `.yunta/workflows/` durante un run: si el equipo edita el workflow a mitad de ejecución, los runs en curso conservan sus reglas. Corolario: un run tampoco cambia de modo — se promueve creando un run sucesor (§10.2).
## 2.2 Layout y potestad: qué vive dónde y quién lo decide
| Ámbito | Ubicación | Contenido | Potestad |
|---|---|---|---|
| Proyecto (versionado) | `.yunta/` en el repo | `config.yaml`, `workflows/`, `skills/`, `knowledge/`, `packs/` • `yunta.lock` | del equipo, vía PR |
| Usuario (global) | `~/.yunta/` | `config.yaml` personal (credenciales, binarios, runners propios) + **estado de ejecución**: `runs/`, `worktrees/`, DB del event log | del usuario |
| Org (sistema) | `/etc/yunta/config.yaml` (Linux/macOS) · `%ProgramData%\yunta\config.yaml` (Windows) · override: `YUNTA_ORG_CONFIG` | defaults corporativos + techo de `permissions` (§6.1) | de la organización |

Dos reglas de potestad distintas, deliberadamente:
- **Configuración**: resolución repo > usuario > org (§2.1), con la inversión de `permissions` (§6.1) como única excepción.
- **Estado de ejecución**: siempre global, **jamás dentro del repo**. Los runs pertenecen a quien los corre — dos personas ejecutando el mismo workflow no comparten worktrees ni logs — y el estado no puede vivir dentro del árbol de git que los propios agentes manipulan (§2: un `git clean` no debe poder alcanzarlo).
**Dónde va el estado es potestad del usuario**: claves `paths.runs` y `paths.worktrees` en config (resueltas por capas como todo), y la variable `YUNTA_HOME` como override de raíz para entornos efímeros (CI con volumen montado, home con poco disco, políticas de ubicación de datos). La ubicación de la **capa org** sigue el mismo patrón — default por plataforma (`/etc/yunta/` en POSIX, `%ProgramData%\yunta\` en Windows, la carpeta convencional de máquina en cada sistema), overrideable por `YUNTA_ORG_CONFIG` — así Windows no es una excepción al esquema sino el mismo mecanismo con otro default. Los paths resueltos se congelan en el manifest del run como todo lo demás: un `resume` jamás busca el run.dir en un lugar distinto del que el run nació, aunque la config haya cambiado después.
## 2.3 Inputs del workflow
Un workflow declara sus inputs como **mapa de nombre a especificación** — el nombre es la identidad, así que el formato garantiza unicidad sin validarla, y es coherente con las demás colecciones nombradas del schema (`runners:`, `modes:`, `mcp_servers:`):
```
inputs:
	idea:
		type: string
		required: true
		description: "What to build — becomes the brief"
	target_branch:
		type: string
		default: "\{\{project.base_branch\}\}"
		pattern: "\^\[a-zA-Z0-9._/-\]+\$"
	severity_floor:
		type: enum
		values: \[blocking, major, minor\]
		default: major
	max_tasks:
		type: number
		min: 1
		max: 200
		default: 40
	changelog:
		type: path            # debe existir al crear el run
```
Tipos: `string`, `number`, `boolean`, `enum`, `path`. `required` y `default` son mutuamente excluyentes — tener default implica no requerido. Validaciones por tipo: `values` (enum), `pattern` y `min_length` (string), `min`/`max` (number). **`path`**** valida existencia siempre**, sin flag y sin distinguir archivo de directorio: un path inexistente va a fallar de todos modos, y hacerlo al crear el run convierte un error caro y confuso — tras worktree, baseline y quizá tokens gastados — en uno inmediato y claro. Quien necesite nombrar un archivo que aún no existe está pidiendo un `string`, no un `path`.
`description` no es decorativa: es lo que `list_workflows` le muestra a un agente cliente y lo que `--help` muestra a una persona. Un catálogo sin descripciones es una lista de nombres sin sentido.
Reglas: todo se valida **al crear el run, antes del primer token**; los defaults se resuelven en ese momento y quedan congelados en el manifest (resolverlos por nodo introduciría estado no determinista); y `yunta check` verifica que todo `{{inputs.x}}` de los templates refiera a un input declarado.
# 3. Modelo de eventos
El event log es append-only: `(run_id, seq, timestamp, node_id?, kind, payload_json, schema_version)`. El estado actual no se guarda: se **deriva** por replay del log (snapshots solo como optimización, jamás como fuente de verdad). Los 30 tipos de evento (24 filas; varias agrupan variantes emparentadas):
| Evento | Emisor | Payload relevante |
|---|---|---|
| `run_created` | engine | manifest hash, inputs, modo, `promoted_from?` |
| `runner_resolved` | engine | rol, candidato elegido, candidatos descartados y causa |
| `baseline_captured` | engine | comando de suite, resultados, hash |
| `node_started` | engine | node_id, intento N |
| `agent_session_opened` | adapter | session_id, agente, modelo, capacidades |
| `agent_message` | adapter | resumen/uso de tokens (nunca el texto completo) |
| `artifact_written` | engine | node_id, path, content hash |
| `context_assembled` | engine | node_id, fuentes resueltas, hash por segmento de estabilidad (§9.1) |
| `task_registered` | engine | task_id, criteria, scope, deps |
| `criteria_checked` | engine | task_id, fase pre/post, exit code por criterio, ejecutado o reutilizado de caché (§5.4) |
| `task_status_changed` | engine | task_id, estado nuevo, evento que lo justifica |
| `scope_checked` | engine | task_id/node_id, diff observado, violaciones |
| `scope_expansion_requested` | engine | task_id, paths, razón, criterio propuesto y su pre-check (§6.2) |
| `scope_expansion_granted` / `scope_expansion_denied` | engine | task_id, decisor (regla o persona), modo, conteo del run |
| `node_finished` / `node_failed` | engine | resultado, tokens, ¿reintentable? |
| `hook_executed` | engine | node_id, fase before/after, comando, exit code |
| `node_rerouted` | engine | nodo fallido, destino, causa, reintento N de M |
| `gate_waiting` / `gate_resolved` | engine/adapter | opciones, elección, quién, feedback |
| `questions_answered` | engine | node_id, hash del artifact de respuestas, canal (tty\\|mcp\\|pr), respondiente si se conoce |
| `loop_iteration` | engine | iteración N, evaluación de `until` |
| `finding_posted` | engine | autor (nodo), hallazgo: id, severidad, título, location, detalle (§4.1) |
| `promotion_signaled` | engine | razón, evidencia, modo sugerido |
| `child_run_created` / `child_run_finished` | engine | node_id, child run_id, `workflow_hash` del hijo, estado terminal |
| `capability_degraded` | engine | capacidad, adapter, política aplicada |
| `run_paused` / `run_resumed` / `run_finished` | engine | razón / estado terminal, métricas |

Dos decisiones incorporadas al modelo. Primera: `agent_session_opened` es obligatorio para los adapters y transporta el session_id — es lo que hace posible reanudar conversaciones (§8.1). Segunda: el uso de tokens viaja en eventos, así que los presupuestos (`limits.*`) se evalúan en el engine contra el log, nunca contra el autorreporte del agente.
## 3.1 Schema de eventos y evolución
Los eventos son el formato más duradero del sistema: sobreviven a los runs, se exportan a `events.jsonl`, alimentan recibos y stats, y deben poder leerse años después. Su evolución se rige por cuatro reglas:
**Versión por tipo, no global.** `schema_version` es la versión *de ese* `kind`: `criteria_checked` puede ir por su v3 mientras `run_created` sigue en v1. Una versión global obligaría a tocar todo cada vez que un solo evento cambia.
**Dentro de una versión, solo cambios compatibles.** Agregar campos opcionales, sí. Renombrar, eliminar o cambiar el tipo de un campo existente, no: eso es un `kind` nuevo (`criteria_checked_v2`), y el lector viejo lo ignora en lugar de romperse. Es deliberadamente burdo — la alternativa, migrar el log al actualizar, introduciría una operación falible sobre la fuente de verdad y contradiría el append-only (I2). En un sistema cuyo argumento es la auditabilidad, un log que jamás se reescribe vale más que un vocabulario prolijo; los `kind` legacy se retiran cuando la retención los agota.
**Lector tolerante, escritor estricto.** Al leer, los campos desconocidos se ignoran — el replay nunca falla por encontrar un evento más nuevo de lo esperado. Al escribir, el payload se valida contra el schema: un evento inválido es un bug del engine, no un warning.
**Evento desconocido: parcial, no roto.** Si el replay encuentra un `kind` que esta versión no conoce (log escrito por una posterior), deriva lo que puede y marca el run como *parcialmente interpretado*, con diagnóstico — ni `broken` innecesario ni silencio (I11).
**Forma.** El JSON Schema se **genera desde los tipos de Rust** y se versiona en el repo: los tipos son la fuente de verdad y el schema es su emisión. Así, cualquier cambio que altere el formato produce un diff visible en el PR — nadie modifica el contrato de eventos sin notarlo — y los golden tests comparan contra el schema emitido, no contra expectativas escritas a mano.
**Normalización en lectura.** Las variantes de versión son un detalle de almacenamiento, jamás vocabulario de usuario: al leer, el engine normaliza cada evento al modelo de dominio actual **en memoria**, sin tocar el disco. Ningún `_vN` aparece en `status`, `stats`, el recibo ni ninguna otra superficie de usuario. El log conserva exactamente los bytes que se escribieron — la evidencia no se reescribe — y quien lo consume ve un vocabulario limpio. Esto también resuelve el `events.jsonl` ya exportado: como los archivos que salieron del sistema no pueden migrarse, la compatibilidad de lectura es la única garantía que funciona para ellos.
## 3.2 Estados de nodo
Derivados del log: `pending → ready → running → done | failed | skipped`, más `waiting` (gates, y nodos cuyas preguntas pendientes esperan respuesta — §4.1). Un nodo pasa a `ready` cuando todas sus dependencias están `done`. `failed` con política `pause` congela el run entero en estado reanudable.

## 3.3 Cadena de hashes del event log

Cada evento persiste un `event_hash` que encadena con el anterior — lo que RFC-0003 promete en el recibo ("342 events, hash-linked, replayable") tiene aquí su definición exacta:

```
event_hash = SHA-256(prev_event_hash \|\| campos_estructurales_en_orden_fijo)
```

**Campos que participan**, en el orden declarado del schema (nunca orden alfabético de mapa — evita ambigüedad de serialización): `run_id, seq, timestamp, node_id, kind, payload_json, schema_version`. El `event_hash` se calcula sobre **los bytes exactamente como se persistieron**, antes de cualquier normalización en lectura (§3.1): la integridad de la cadena es así completamente ortogonal a la evolución del schema — un evento `_v2` futuro no invalida hashes ya calculados sobre eventos `_v1` existentes.

**Génesis determinístico, sin null ambiguo**: `H0 = SHA-256(manifest_hash)`. Atarlo al manifest (que ya existe, §2.1) hace que la cadena nazca única por run sin inventar una constante nueva, y liga la integridad de eventos a la integridad del propio run desde el primer eslabón.

**El hash se persiste junto al evento, nunca se recalcula on-demand en cada replay** — hacerlo sería O(n) hashes en cada resume de un log largo, contra el espíritu de que el replay sea barato. La **verificación** (recorrer la cadena y recalcular para detectar alteración) es una operación aparte, explícita: corre automáticamente al generar el recibo — no sería honesto que el recibo afirme "hash-linked" sin haberlo comprobado en ese momento (mismo principio de I20) — y está disponible bajo demanda vía `yunta verify <run_id>` para cualquiera que la quiera correr sin generar el recibo completo. Una cadena rota (payload alterado, evento borrado, insertado, reordenado, o `prev_event_hash` alterado) marca el run como `broken` con diagnóstico exacto de dónde se rompió — mismo tratamiento que un log insuficiente para derivar estado (§8.1).

Esto es **integridad y orden, no autenticidad**: la cadena prueba que nadie alteró el log después de escrito, no quién lo escribió. La firma criptográfica (A-06) es la capa que agrega autoría, y queda deliberadamente separada — son garantías distintas y no hay que confundirlas.

# 4. Artifacts
Un artifact es un archivo bajo `artifacts/` que un nodo **declara** producir (`artifacts.produces`). Al terminar el nodo, el engine verifica existencia y no-vacuidad; si falta, el nodo es `failed` sin importar qué haya reportado el agente. **El engine no asume formato**: verifica existencia y hash de cualquier archivo — un PDF, una planilla, una imagen o un dump son artifacts tan válidos como un markdown. Un límite configurable (`limits.max_artifact_bytes`) actúa como guardia contra accidentes: al excederse, el nodo falla con diagnóstico en lugar de degradar el run en silencio. Los artifacts son **inmutables**: una vez emitido `artifact_written` con su hash, cualquier diferencia posterior es corrupción detectable. El progreso de un run nunca se expresa editando documentos — se expresa como eventos (§5). Un plan es una foto congelada; el avance es una secuencia auditable.
## 4.1 Artifacts opacos e interpretados
Por default un artifact es **opaco**: el engine sabe que existe y qué hash tiene, y su estructura interna es la que el agente haya decidido — dos runs del mismo workflow pueden producir formatos distintos y ambos son válidos. Eso alcanza para todo lo que el engine transporta sin necesitar entender.
El campo `kind` marca lo contrario: que el engine **interpreta** el contenido. No describe el formato del archivo, declara que hay un parser, un schema y una validación detrás, y que un archivo mal formado falla el nodo. La vara para agregar un `kind` es alta: solo cuando el engine necesita los datos para decidir o contar, nunca cuando alcanza con verificar que el archivo está.
**La forma de un `kind:` se publica a quien la escribe, y un archivo ilegible se corrige.** Un `kind` declara que hay un parser detrás; declararlo sin mostrar la forma deja al que escribe adivinando una gramática que el engine tiene en tipos. Un nodo que declara `artifacts.produces: [{kind: <k>}]` recibe la forma de `<k>` como fuente de contexto derivada de esa misma declaración, en el segmento `stable` y con la ruta absoluta donde el engine la va a verificar. Fuera de un run la misma forma sale por la tool `document_shape` del plano de control y por `yunta schema <kind>`; las cuatro puertas rinden de una sola constante por kind, con un test que la lee de vuelta por el mismo parser. Cuando aun así el archivo no se puede leer, el nodo falla `retryable: true` y el engine reabre una sesión con el diagnóstico, contra `limits.max_artifact_repairs`; cada reparación es un intento propio en el log. Lo que una reescritura no arregla — un artifact ausente, vacío o por encima de `limits.max_artifact_bytes` — falla directo.
**`kind: task-ledger`** (§5) — el engine lo parsea, valida y convierte en tareas que viven en el event log.
**`kind: findings`** — hallazgos estructurados. Cada entrada declara `id`, `severity` (`blocking | major | minor | note`), `title`, `location` (path y rango opcional) y `detail`; opcionalmente `proposed_criterion` para los que ameriten volverse tarea. Al cerrarse el nodo, el engine parsea el archivo y emite un `finding_posted` por entrada — con lo cual los hallazgos dejan de ser prosa que alguien debe interpretar y pasan a ser datos del run: se cuentan, se agrupan por severidad, se deduplican entre reviewers por `location` + título normalizado, aparecen en `status` y en el recibo (*"3 findings: 1 blocking, 2 minor"*), y sobreviven al run para el gate de promoción o la destilación. Un nodo consolidador sigue existiendo para el juicio — qué corregir, qué diferir — pero recibe datos, no tres documentos con estructuras distintas.
**Un solo schema de hallazgo, dos vías de entrada.** El mismo formato rige para los findings que un nodo produce como artifact al cerrar y para los que un agente reporta **en caliente** durante su trabajo (tool `yunta_post_finding` del MCP por-run, §6). No hay dos calidades según el momento: un hallazgo de review y uno encontrado a mitad de una tarea son datos homogéneos, se cuentan juntos, se deduplican juntos y se consultan igual. La vía en caliente valida contra el mismo schema y rechaza entradas incompletas — reportar mal es un error visible, no un texto libre que después nadie puede procesar.
Destino: todo finding vive en el event log y en el `events.jsonl` que el run exporta al cerrar (§8.3), sin depender de que el workflow declare nada. El engine **no impone** un artifact de cierre ni un formato de reporte: qué hacer con los hallazgos — consolidarlos, promoverlos, destilarlos, ignorarlos — es decisión del workflow. Lo que el engine garantiza es que no se pierdan y que sean consultables.
**`kind: questions`** — preguntas para una persona. Cada entrada declara `id`, `text`, `answer_type` (`text | choice | boolean`), `values` cuando es `choice`, y `required`. Un nodo que necesita información del usuario **no conversa**: escribe este artifact y termina. El engine lo lee y lo renderiza — en terminal con TTY, pregunta por pregunta; sin TTY o desde MCP, como el mismo objeto que ya renderizan los gates (§5.3), respondible por consola, tool MCP o pull request. Las respuestas se materializan como artifact y emiten `questions_answered` (hash del artifact, canal, respondiente si se conoce) — auditable igual que un `gate_resolved`; el nodo siguiente las consume como contexto normal.

Consecuencias del diseño: no hace falta ninguna capacidad de adapter — todo CLI sabe escribir un archivo, así que ninguno degrada; el run sigue siendo reanudable y desatendible, porque si muere durante la espera las preguntas están en disco y se vuelven a hacer al reanudar, sin conversación a medias que reconstruir; y en CI, sin nadie que responda, el run queda `waiting` con las preguntas registradas, igual que ante un gate. `interactive: true` deja de ser un modo de ejecución y pasa a ser lo que siempre fue: **un dato de presentación** — cómo el engine muestra las preguntas, no cómo corre el nodo.

El conocimiento durable (`knowledge/`, §9.2) permanece deliberadamente **opaco**: su valor es ser prosa que una persona y un agente leen: estructurarlo lo empobrecería sin darle al engine nada que necesite decidir.
# 5. Ledger de tareas: criterios ejecutables y verificación en rojo
Los workflows de implementación giran alrededor de un **ledger**: un artifact estructurado que enumera tareas verificables. Un nodo de planificación lo declara (`artifacts.produces: [{name: plan.yaml, kind: task-ledger}]`); al cerrarse el nodo, el engine lo parsea, lo valida (schema, grafo acíclico, criteria y scope presentes en toda tarea, scopes disjuntos entre tareas paralelizables) y registra cada tarea (`task_registered`). Desde ese momento **las tareas son datos del engine y su estado vive en el event log** — el archivo queda congelado como cualquier artifact. No existe un "archivo de progreso" que mantener ni proteger: no hay nada que adulterar.
## 5.1 Formato de tarea
```
tasks:
	- id: T001
		title: "Extraer middleware de auth"
		depends_on: \[\]
		scope: \["src/auth/"\]           # globs que la tarea puede tocar — obligatorio
		criteria:                        # ejecutables; exit 0 = pasa
			- cmd: "test -f src/auth/[middleware.rs](http://middleware.rs)"
			- cmd: "cargo test -p auth"
			- cmd: "! grep -rn 'auth_legacy' src/"
				type: guard                  # guard: debe pasar antes Y después
		notes: ""                        # contexto mínimo para un runner sin historial
```
Toda tarea requiere al menos un criterio `cmd`. Lo no verificable por comando se reformula hasta que lo sea, o excepcionalmente se marca `manual_review: true` con justificación — el único punto del ciclo donde un LLM juzga completitud, acotado a un nodo de auditoría con rúbrica fija.
Los criterios deben ser **deterministas respecto del árbol de trabajo**: dado el mismo árbol, el mismo resultado. Un comando cuyo veredicto depende de la hora, la red o un servicio externo no es un criterio — es un nodo `bash` (los nodos nunca se memoizan, §5.4), donde además queda visible en el DAG con su evento y su re-ruta, en lugar de escondido en un ledger de doscientas tareas. `yunta check` no puede probar determinismo, pero el ciclo lo delata: un criterio no determinista produce pre-checks azarosos y rebotes inexplicables.
## 5.2 Ciclo de vida de una tarea (ejecutado íntegramente por el engine)
1. **Elegibilidad**: `pending` con todas sus deps `done` (derivado del log).
2. **Pre-check en rojo**: el engine ejecuta los criterios (con memoización y short-circuit, §5.4). Los no-`guard` deben FALLAR — un criterio que ya pasa antes del trabajo no prueba nada, y la tarea rebota a re-plan con evento que identifica el criterio trivial. Los `guard` deben pasar (son la línea de no-regresión local). Esta fase valida al validador: sin ella, un plan de criterios vacuos produciría un run verde donde no se hizo nada, y el post-check no podría distinguir "lo logré" de "ya estaba".
3. El engine lanza el nodo de implementación con brief mínimo: ruta al ledger + task_id. El agente lee **su** tarea en el momento; no recibe el plan como prosa ni historial conversacional.
4. **Post-check**: criterios de nuevo — todos verdes — más scope check (§6). Solo entonces el engine emite `task_status_changed: done`. **No existe API por la cual un agente marque estado de tarea**: la verificación no es un rol que alguien cumple, es una fase que el engine ejecuta.
5. Fallo → reintento (cap configurable, default 2, siempre con sesión nueva), luego pasada de diagnóstico si el workflow la define, luego `blocked` + gate de escalación (§5.3).
Los loops con `until: all_tasks_complete` consultan este estado derivado. Un plan de 200 tareas y uno de 2 usan exactamente el mismo mecanismo: el ledger nunca se resume, se trunca ni se colapsa por tamaño — la implementación itera tarea por tarea desde datos, no desde memoria.
## 5.3 Gate de escalación (cuestionario)
Cuando el sistema necesita una decisión humana por agotamiento — tarea `blocked` (§5.2 paso 5), re-rutas agotadas (§11.2), límite de presupuesto (§8.3) — el gate llega empaquetado: **el que escala hace el trabajo de armar la decisión, no el que decide**. Estructura normativa:
```
escalation:
	summary: "T007 failed 3 times: the auth middleware test expects a session store that doesn't exist"
	evidence:                        # mecánica, adjuntada por el ENGINE desde el log
		- criteria_checked: T007 post (attempt 3) — 2/3 green, failing: cargo test -p auth
		- node-output: tests digest (last 20 lines)
	options:                         # de juicio — las redacta el nodo de diagnóstico
		- id: add-store
			label: "Add an in-memory session store"
			tradeoff: "Unblocks now; +1 task to ledger; touches src/session/ (outside current scope)"
		- id: descope
			label: "Defer T007, ship without session persistence"
			tradeoff: "Ships today; creates known-gap finding for next run"
		- id: abort
			label: "Abort the run"
	free_text: true                  # siempre disponible
	default_on_timeout: none         # jamás auto-decide; esperar es un estado válido
```
Reglas: la `evidence` la adjunta el engine directo del log — el humano audita en el mismo gate si el `summary` del LLM es fiel a los hechos certificados; toda opción lleva `tradeoff` obligatorio y las que amplían trabajo lo declaran — elegirlas autoriza la ampliación, que el engine registra en `gate_resolved` y traduce en re-plan o promoción; `free_text` siempre existe — el menú acelera el caso común, no encierra; ningún timeout decide — auto-decidir sería degradación silenciosa con otro nombre. El mismo objeto se renderiza en toda superficie (consola, tool MCP `resolve_gate`) vía el trait `HumanInteraction`; sus textos son de cara al usuario y van en inglés.
## 5.4 Memoización de criterios
El engine no reejecuta un comando cuyo resultado ya conoce. Antes de correr un criterio calcula una clave que captura **todas** sus entradas:
```
key = hash(comando + tree_hash + env declarado + versión de config resuelta)
```
`tree_hash` es el hash del árbol de trabajo (git lo calcula gratis: árbol del índice, o commit más diff sucio). Si nadie modificó un byte desde la ejecución anterior, la clave coincide y el resultado se reutiliza; si algo cambió — una edición del agente, un hook que corrió un formatter, un `before` que instaló dependencias — la clave cambia y el criterio se ejecuta de verdad. La clave *es* el estado: no existe forma de que un cambio pase inadvertido.
Esto elimina la única redundancia real del ciclo: los criterios `guard` (suite global, baseline) que el post-check de una tarea acaba de ejecutar y el pre-check de la siguiente volvería a pedir. Los criterios propios de cada tarea tienen comando distinto — clave distinta — y se ejecutan siempre, que es exactamente lo que debe pasar.
Reglas: la memoización vive **dentro del run** (nunca cross-run: otra máquina, otro entorno u otro día invalidan las suposiciones); no hay opt-out por criterio, porque los criterios son deterministas por definición (§5.1) y lo no determinista pertenece a nodos `bash`, que nunca se memoizan; y `criteria_checked` registra si hubo ejecución o reutilización, de modo que recibo y replay muestran qué se corrió y qué se reutilizó — nada se da por verificado en silencio.
Complemento del pre-check: **short-circuit con orden aprendido**. Basta que un criterio no-`guard` falle para que la fase concluya; y el engine ordena los criterios de menor a mayor duración histórica (dato que el log ya tiene de ejecuciones previas del mismo comando), de modo que el pre-check típico termina en el primer comando barato en rojo sin llegar a la suite. La heurística solo afecta el orden de evaluación — nunca qué se verifica ni el veredicto.
## 5.5 Ejecución paralela de tareas
Un loop de implementación puede ejecutar varias tareas a la vez:
```
- id: implement
	kind: loop
	until: all_tasks_complete
	concurrency: 4        # tareas simultáneas; default 1 (secuencial)
```
**Formación del lote.** El engine toma hasta N tareas `ready` cuyos scopes sean disjuntos entre sí — la validación de scopes del §5 deja de ser advertencia y pasa a ser criterio de agrupamiento. Si el ledger es una cadena de dependencias, el lote es de 1 y el comportamiento coincide con el secuencial: no hay caso especial.
**Aislar para trabajar.** Cada tarea del lote recibe su propio worktree derivado del commit base actual. Trabaja sola: su `git diff` contiene solo lo suyo, su tree_hash es estable y la memoización (§5.4) sigue siendo válida. Sin esto, dos runners sobre un mismo árbol se invalidan los checks mutuamente y ni el scope ni la caché significan nada.
**Serializar para verificar.** Verde en el árbol individual es condición necesaria, nunca suficiente. El engine integra las tareas **de a una y en orden de declaración del ledger** — no en orden de finalización: el orden por timing no sería reproducible, y el mismo ledger debe producir la misma secuencia de commits. Cada integración **rebasa** la tarea sobre el estado actual (que puede haber cambiado por una integración previa) y **reejecuta sus criterios ahí**; el veredicto que cuenta es siempre el post-integración. Solo entonces `task_status_changed: done` y commit. Si el rebase o los criterios fallan en integración, esa tarea vuelve a `ready` sobre el árbol nuevo — ninguna otra del lote se ve afectada.
**Guards y suites globales** se ejecutan una vez por lote integrado, no por tarea: la memoización lo resuelve sin lógica extra — mismo comando, mismo árbol post-integración, un solo hit. De lo contrario el paralelismo se comería su propia ganancia.
**Presupuesto y resume.** El engine reserva presupuesto por tarea al formar el lote: N sesiones simultáneas consumen más rápido y el cap del run se sigue respetando. Reanudar a mitad de lote no tiene caso especial: cada tarea es independiente en el log, las que quedaron `running` huérfanas se reejecutan y su worktree individual se descarta y renace — no hay estado compartido que reconstruir.
**Default 1, deliberado.** El paralelismo multiplica el gasto simultáneo de tokens; nadie debe descubrirlo por la factura. Se declara explícitamente, y `stats` reporta la ganancia real de wall-clock para que subirlo sea una decisión con datos.
## 5.6 Gates externos: resolución por pull request
Un gate puede resolverse **fuera de Yunta**, delegando en la forja del equipo el sustrato multi-persona que el engine no provee en v1 (identidad, permisos, notificaciones y estado compartido ya existen ahí):
```
- id: approve-spec
	kind: gate
	assignee: arquitectura
	external:
		kind: pull_request
		artifacts: \[[spec.md](http://spec.md)\]        # qué se publica para revisar
		branch: "\{\{run.branch\}\}"
```
Al llegar al gate, el engine **publica**: commitea los artifacts declarados en la rama, abre un PR cuyo cuerpo lleva el summary del gate, el `run_id` y el enlace al run, y emite `gate_waiting` con la URL. Ahí termina — sin proceso corriendo, como cualquier `waiting`. La persona que aprueba **no necesita Yunta instalado ni acceso a la máquina del run**: revisa un PR normal, comenta y decide donde el equipo ya trabaja.
La resolución es **pull, no push**: no hay webhooks ni daemon: el engine consulta el estado del PR cuando alguien lo despierta (`resume`, `status`, o un job programado del CI). Así el modelo "sin infraestructura" queda intacto. Mapeo: aprobado → el gate continúa; cambios pedidos → los comentarios entran como `finding_posted` y el gate ofrece la re-ruta declarada; PR cerrado sin mergear → abort; PR mergeado → el gate continúa como aprobado por quien mergeó, con el SHA del merge como evidencia. Los comentarios se montan como contexto del nodo correctivo, igual que `node-output`: quien corrige lee lo que la persona escribió, sin transcripciones intermedias.
La evidencia sigue siendo mecánica: `gate_resolved` registra la respuesta de la API — usuario, timestamp y **SHA aprobado** — no la afirmación de nadie; si el PR cambió después de la aprobación, el engine lo detecta por SHA y el gate vuelve a esperar. Sin credenciales de forja o sin conectividad, el gate degrada a consola con aviso explícito, nunca cuelga ni asume; `yunta check` valida que todo gate `external` tenga forja configurada.
Límite honesto: esto desbloquea runs desde cualquier lado, pero no los **avanza** — ejecutar el siguiente nodo sigue siendo de quien tiene el estado local. "Cualquiera del equipo corre cualquier run desde cualquier máquina" requiere estado compartido, y eso pertenece al proyecto de servidor de equipo, fuera de Yunta.
## 5.7 Re-plan: qué sobrevive a un ledger nuevo
Un nodo de planificación puede volver a ejecutarse — porque una tarea rebotó por criterio trivial (§5.2 paso 2), porque un gate eligió ajustar, o por una re-ruta — y producir un ledger distinto del que ya generó tareas, algunas de ellas `done` con su trabajo commiteado.
Regla: **una tarea conserva su estado ****`done`**** solo si su identidad verificable no cambió** — mismo `id`, mismos `criteria` y mismo `scope`. Cualquier diferencia la devuelve a `pending`. El criterio no es la prolijidad del planificador con los identificadores: `done` significa "sus criterios pasaron", así que si los criterios cambiaron, el `done` anterior no dice nada sobre la tarea nueva. Conservar por `id` a secas sería peligroso — un planificador puede reusar `T003` para algo completamente distinto y el engine lo daría por hecho — y descartar todo sería tirar trabajo ya verificado.
El trabajo commiteado no se revierte: el worktree conserva lo hecho, y las tareas invalidadas vuelven a correr sobre ese estado — su pre-check en rojo dirá si seguían siendo necesarias. Todo el rebalanceo queda visible: el engine emite `task_status_changed` por cada tarea invalidada con el re-plan como causa, y el recibo lo declara (*"re-plan at node plan: 3 tasks preserved, 2 invalidated"*). Un re-plan nunca descarta trabajo en silencio.
## 5.8 `kind: parallel`: nodos estáticos y su join

`parallel` corre nodos **conocidos de antemano** — los que el autor escribió a mano en el workflow, con id propio cada uno. No confundir con `concurrency` (§5.5): `parallel` es para "corré estos nodos puntuales que yo nombré, a la vez"; `concurrency` es para tareas de un ledger cuyo número no existe hasta que el plan corre, y trae garantías que `parallel` no necesita (worktree por tarea, integración serializada, memoización de guards). Si el número de unidades se puede contar mirando el YAML, es `parallel`; si depende de lo que el plan genere en runtime, es `concurrency`.

```
- id: pre-launch
	kind: parallel
	join: all              # all (default) \| any
	nodes:
		- \{ id: write-docs, kind: prompt, ... \}
		- \{ id: load-test, kind: bash, ... \}
```

**`join`** define cuándo el grupo termina. `all` (default): el grupo completa cuando completan todos los hijos; si uno falla, el grupo falla — formaliza el comportamiento que antes era implícito. `any`: el grupo completa con el primer hijo que termina con éxito; a los demás el engine les envía `interrupt` (mismo mecanismo de cancelación ordenada de la Spec del Adapter, escalando a `kill` si no cierran a tiempo).

**Colisión de escritura entre hermanos.** Los hijos de un `parallel` comparten un único worktree (a diferencia de `concurrency`, que aísla por tarea) — así que dos hijos que escriben pueden pisarse. Lo que el engine puede garantizar depende de qué se declaró: si **ambos** hijos declaran `scope` y se solapan, es **error** en `check`, antes de correr nada (verificable estáticamente, igual que en §5.1). Si **no** declaran scope, el engine honestamente **no puede detectar la colisión**: no existe manera de atribuir qué archivo tocó cada hijo cuando corren de verdad en simultáneo sobre el mismo árbol — ningún nodo `bash` tiene tracking de escritura en caliente, y `edit_hooks` es una capacidad de adapter de agente, no de procesos arbitrarios. Por eso `check` emite **warning** (no error, mismo registro que D48) cuando un grupo tiene dos o más hijos con permisos de escritura (`edit`/`full`, o `bash` sin restricción) y sus scopes no están declarados como disjuntos — recomendando declarar `scope` como única protección real disponible ahí. No hay detección automática de colisión sin scope: prometerla sería inventar un dato que el sistema no puede producir.

## 5.9 Blackboard: posteo siempre, lectura por fase

Con `coordination: blackboard` (D49), un nodo puede **postear** en cualquier momento de su ejecución — `yunta_post_finding` está siempre disponible para todo nodo con `run_tools` (§6.4). Pero **leer** el blackboard de otros nodos del grupo está restringido a después del `join`: mientras el grupo sigue corriendo, `yunta_get_blackboard` no devuelve posteos de hermanos, solo los propios si los hubiera.

La razón es determinismo de corrida, no de replay: dos ejecuciones del mismo workflow con inputs idénticos pueden tener sesiones cuyo timing real difiere, y si un hermano pudiera leer en caliente lo que otro fue posteando, el resultado dependera de en qué orden llegaron los posts — no solo de su contenido. Al cerrar el join, el engine consolida todos los posteos del grupo en un artifact/evento de cierre, consumible por un nodo **posterior** al `parallel` (`context: [{ node-output: { node: <parallel_id> } }]`) — nunca entre hermanos en caliente. La coordinación cooperativa sigue resuelta; ocurre después del grupo, no adentro.

Esto es distinto de por qué `independent` es el default de §6.4: ahí la razón es sesgo de anclaje (un reviewer que ve el hallazgo de otro deja de mirar con ojos frescos), específico de grupos evaluativos. `blackboard` sigue siendo para grupos cooperativos, donde no hay juicio que anclar — la restricción de esta sección es sobre *cuándo* se puede leer, no sobre si conviene ver lo ajeno.

# 6. Scope: definición mecánica de drift
El scope existe porque el ejecutor es **no determinístico**. Un agente puede decidir tocar algo que nadie le pidió, y la única forma de saberlo es comparar lo que hizo contra lo que podía hacer. Por eso el scope es obligatorio en las tareas del ledger — donde hay un agente decidiendo — y no en los nodos determinísticos: el alcance de un comando es el que su autor escribió, y verificarlo sería overhead sin información. Un nodo `bash` o `executor` puede declarar `scope` si su autor lo quiere (el engine lo verifica igual), pero no declararlo no produce error ni warning: exigirlo saltaría en casi todo nodo legítimo — `git push`, `gh pr create`, correr una suite — y un aviso que salta siempre es ruido que se aprende a ignorar. Quien quiera exigirlo tiene la palanca correcta en `permissions` (§6.1), donde es una política elegida y no un default molesto.
Cada tarea (y opcionalmente cada nodo) declara `scope` como globs. Sin scope declarado, el desvío es opinable; con scope, es computable. Verificación en dos niveles:
- **Post-nodo (garantizado)**: el engine computa `git diff --name-only` desde el inicio de la tarea y lo contrasta con los globs. Archivos fuera → tarea `failed` con la lista completa (`scope_checked` con violaciones).
- **En caliente (mejor esfuerzo)**: si el adapter declara la capacidad `edit_hooks`, el engine le pide bloquear ediciones fuera de scope en el momento en que ocurren. Capacidad declarable en el trait `Adapter`; si no está, degradación explícita a solo-post-check con warning.
El scope hace además verificable la no-colisión del paralelismo: tareas simultáneas requieren scopes disjuntos, validado al registrar el ledger.
Lo que un runner encuentra fuera de su scope no se parchea ni se ignora: se reporta como finding con el schema de §4.1 — vía `yunta_post_finding` o, automáticamente, cuando se le deniega una ampliación (§6.2) — y queda en el event log del run, disponible para el gate de promoción (§10.2), para la destilación al cierre o para consulta posterior. Diferir un hallazgo es una decisión registrada con evidencia y fecha, nunca un olvido.
## 6.1 Permisos: modelo unificado
`permissions` es UN modelo con varios niveles, no mecanismos sueltos. Semántica común a todos: **cada nivel es un techo; los niveles inferiores solo pueden estrechar, jamás aflojar.** La escalera: org → repo/usuario → pack (`declares.permissions`) → nodo (`permissions: read-only|edit|full`) → scope de tarea (globs). El scope (§6) es el peldaño más fino del mismo modelo.
Niveles org/repo en config — nótese la **inversión de precedencia deliberada**: para todo lo demás la config resuelve repo > usuario > org; para `permissions` la capa org manda y las inferiores solo restringen más (sin esta inversión, la gobernanza es teatro: cualquier repo la anularía):
```
# /etc/yunta/config.yaml (capa org — techo)
permissions:
	commands:
		deny: \["curl * \| *", "wget * \| *", "sudo \*"\]   # patrones sobre bash/hooks/criterios/executors
		allow: \[\]                                       # vacío = todo lo no denegado (denylist default)
	packs:
		executors: prompt                               # allow \| prompt \| deny
		publishers: \{ allow: \[acme, internal\] \}         # vacío = todos
	network:
		default: true                                   # false = sin red salvo declaración explícita
```
Enforcement en dos momentos: `yunta check` atrapa lo estático (el comando escrito en el YAML, el pack que excede su techo), y el engine valida **en runtime** cada comando de hook/criterio/bash/executor contra el modelo justo antes de ejecutarlo — un template puede construir en runtime lo que el YAML no mostraba. Violación en runtime = nodo `failed` citando la regla, con evento.
Límite honesto, normativo: esto es **gobernanza, no sandbox**. Un agente con permisos de escritura puede rodear un patrón textual escribiendo un script y ejecutándolo. El modelo detiene el accidente y el pack descuidado, y deja rastro auditable del intento deliberado; el aislamiento real (container, VM) pertenece al entorno de ejecución, no a Yunta. Prometer más sería seguridad aparente — peor que ninguna.

**`network` en particular es declarativa, no un enforcement del engine.** A diferencia de `commands` (que el engine sí compara contra patrones antes de ejecutar, esta sección arriba), `network: false` no activa ningún sandboxing de red — Yunta core no tiene ni promete un mecanismo que aisle a un proceso arbitrario de la red del sistema operativo. Es una **declaración de intención** que el engine puede usar para política y auditoría (un pack que declara `network: false` y después un nodo suyo hace `curl` es una contradicción detectable, no una fuga bloqueada), y que un `executor` que quiera hacerla cumplir de verdad puede implementar por su cuenta (namespaces, un contenedor, lo que el ejecutor elija) — eso es responsabilidad de ese executor, nunca una garantía del core. Tres capas que no se confunden: **policy** (lo que el YAML declara) ≠ **capability** (lo que un executor concreto puede hacer cumplir) ≠ **OS enforcement** (una garantía física, si el entorno de ejecución la provee — nunca Yunta por sí sola). Un usuario que lea `network: false` y asuma sandbox real está leyendo una garantía que el sistema nunca ofreció.
## 6.2 Ampliación de scope: el agente propone, el engine dispone
Entre "esto no me corresponde" (`finding_posted`) y "esto excede el modo" (`promotion_signaled`) existe un caso frecuente: un arreglo chico, adyacente, que sale más barato hacer ahora que registrar y retomar después. Ampliar el scope por decisión propia sería la puerta trasera al drift, así que la ampliación se **solicita** y se **concede**:
```
- id: implement
	kind: loop
	scope_expansion:
		mode: ask                    # rules \| ask \| deny (default)
		within: \["src/"\]           # techo: jamás fuera de esto
		max_per_run: 3
```
**La solicitud es un objeto único, idéntico en los tres modos** — el agente entrega siempre lo mismo, cambia quién decide: paths pedidos, razón, criterio verificable propuesto, y qué pasa si se deniega. Que la información no dependa del destinatario evita que existan dos calidades de decisión.
**Modos.** `rules`: el engine concede si se cumplen las condiciones declaradas (dentro de `within`, tamaño acotado, criterio presente y en rojo). `ask`: se resuelve como gate (§5.3) por consola, MCP o PR. `deny` (default): no hay ampliaciones — toda solicitud se vuelve `finding_posted` sin interrumpir. El modo puede endurecerse desde capas superiores y nunca aflojarse, como todo `permissions` (§6.1).
**El engine agrega lo que el agente no puede saber ni debe autoevaluar**: si el path está dentro de `within`, cuántas ampliaciones lleva el run, si otra tarea toca ese path, el tamaño del diff propuesto, y — clave — **el resultado de correr el criterio propuesto**: si ya pasa, es trivial y la solicitud se rechaza sin consultar a nadie (misma lógica del pre-check en rojo, §5.2).
**Nada es invisible.** `scope_expansion_requested` y `scope_expansion_granted|denied` registran solicitud, decisor y modo; el conteo vive en el run (`max_per_run`, cuyo agotamiento pausa con escalación — diez concesiones seguidas no son readecuación, son un plan mal cortado); y el recibo lo declara: *"1 expansion granted (task T007, +2 −2, authorized by …)"*. Una ampliación concedida no borra el scope original: el diff se evalúa contra scope declarado más ampliaciones autorizadas, cada una con su traza.
**Toda denegación deja finding, en cualquier modo.** Cuando una solicitud se rechaza — por regla, por persona, por cap agotado o por modo `deny` — el engine la convierte automáticamente en `finding_posted` con el schema de §4.1, usando la razón y el criterio propuesto que el agente ya escribió. Esto cubre el caso más frecuente de hallazgo en caliente sin depender de que el agente además se acuerde de reportarlo: ya está pidiendo permiso, la evidencia ya está armada. Lo encontrado no se pierde por haberse denegado el arreglo.

## 6.3 Secretos: env vars y nada más

Los secretos llegan a un agente únicamente por las env vars que el manifest declara; el engine las redacta de todo payload y jamás entran al event log (I12). **Yunta no integra gestores de secretos** — ni 1Password, ni Vault, ni el de una nube — y no es una omisión: el gestor que el equipo ya usa puebla el entorno antes de invocar el binario (`op run -- yunta run …`, `vault exec …`, lo que corresponda), y Yunta consume env vars como cualquier otra herramienta de línea de comandos.

Integrarlos significaría mantener un adapter por gestor, cada uno con su autenticación, y poner al engine en el negocio de custodiar credenciales — exactamente lo que convierte una herramienta sin infraestructura en algo que un área de seguridad debe auditar. La composición con el gestor existente es más simple, más auditable y funciona con cualquiera, incluso con los que todavía no existen.

## 6.4 MCP: superficie de control y por-run

### Superficie de control (`yunta mcp`)

Tools: `list_workflows`, `run_workflow`, `resume_run`, `resolve_gate`, `workflow_status`. **Ninguna bloquea por la duración del run.** `run_workflow` crea el run y retorna de inmediato con `run_id` — internamente dispara `yunta run --detach`, un proceso **desacoplado** de la sesión MCP que sigue vivo aunque el cliente MCP cierre: un run nunca depende de la vida de ningún proceso en particular (§1), y `yunta mcp` en sí mismo no es un daemon (§6) — si `run_workflow` bloqueara o el run muriera con la sesión, sería un daemon disfrazado mientras dura el run. El agente cliente hace seguimiento del progreso llamando `workflow_status(run_id)` — **pull, sin notificaciones push**, mismo modelo que los gates externos (§5.6). `resolve_gate` y las respuestas a `kind: questions` (§4.1) son llamadas de control independientes, no parte de la sesión que creó el run.

### MCP por-run

La comunicación entre nodos es siempre mediada por el engine (§12, D26): nunca directa entre agentes. Durante su sesión, un nodo con la capacidad `run_tools` recibe un endpoint MCP **por-run** con cuatro tools:

- **`yunta_post_finding`** — reporta un hallazgo con el schema de §4.1; disponible para **todo** nodo con `run_tools`, no solo en grupos paralelos (§6).
- **`yunta_request_scope_expansion`** — emite la solicitud de §6.2.
- **`yunta_task_status`** — consulta de solo lectura del estado del ledger, equivalente a la fuente de contexto `ledger` pero invocable en caliente.
- **`yunta_get_blackboard`** / posteo implícito de `yunta_post_finding` — el **blackboard**: solo se monta cuando el nodo pertenece a un grupo `parallel` con `coordination: blackboard` (D49). Es append-only y **scopeado al grupo**: un nodo de un grupo distinto, o de un grupo `independent`, no recibe la tool ni puede leerlo. Cada posteo al blackboard es un `finding_posted` mediado por el engine — mismo evento, mismo schema — con el `node_id` del autor como único dato de scoping adicional; no existe un evento separado para el blackboard porque no es un canal distinto, es una vista filtrada del mismo mecanismo de findings.

Nada de esto abre un canal directo agente-a-agente: cada llamada pasa por el engine, queda en el event log, y respeta las mismas reglas de auditoría que el resto del contrato.

## 6.5 Transporte y ciclo de vida del MCP por-run

**Transporte: HTTP sobre loopback, con puerto efímero y token bearer** — no Unix sockets ni named pipes, para que el mecanismo sea idéntico en Linux, macOS y Windows sin ramas de código por plataforma; y HTTP porque es lo que los CLIs de agentes ya saben hablar como cliente MCP externo, sin inventar un protocolo propio.

**Quién arranca qué.** El **engine** es el servidor: antes de invocar `Adapter::spawn()` para un nodo cuyo runner resuelto declara `run_tools`, levanta un listener efimero y genera una credencial de un solo uso (token de alta entropía), y los pasa en `SessionRequest.run_tools_endpoint`. El **adapter** traduce ese endpoint al mecanismo nativo de su CLI para conectarse a un servidor MCP externo (mismo patrón que `agent:` o `edit_hooks`: el engine no sabe cómo cada CLI se conecta, el adapter sí). El agente, dentro de su sesión, es el cliente.

**Ciclo de vida: por sesión de nodo, no por run.** El listener nace justo antes de esa sesión y muere con ella — sesión terminada (evento terminal, §4 de la Spec del Adapter), interrumpida o `kill`eada, el endpoint se cierra. Un `resume` que reinicia un nodo genera un listener y una credencial **nuevos**; nunca reutiliza los de un intento anterior, mismo principio que `agent_session_opened` numera intentos (§3). Esto es deliberadamente más angosto que "por run": una credencial que sobrevive a su sesión es superficie de reuso indebido.

**Los datos no viven en el listener.** El listener es un gateway delgado: lo que expone (blackboard, task status, findings) vive en el storage del run — el mismo event log de siempre —, no en memoria del proceso servidor. Por eso el blackboard sigue legible después del `join` (§5.9) aunque los listeners de las sesiones que postearon ya hayan muerto: la persistencia es del engine, la conexión es efimera.

**Scoping por construcción, no por parámetro.** La credencial encapsula `(run_id, node_id, intento N)` en el momento de emitirse; ninguna tool acepta un `run_id` como argumento del llamador — el engine siempre lo deriva del token que autentica la llamada. Por diseño, no existe ni puede existir una tool tipo "leé el run que yo te diga": la sesión del run A no tiene forma de nombrar al run B, ni con un token robado le serviría para nada fuera de su propio scope.

**Crashes.** Adapter cae en medio de una llamada: mismo tratamiento que cualquier muerte de sesión sin evento terminal (O2 de la Spec, sintetiza `Failed`); el listener se cierra como parte de la misma limpieza. El proceso del run entero cae (§8.1): al hacer `resume`, el nodo huérfano se retoma según su `on_interrupt`, y si vuelve a correr, nace con listener y credencial nuevos — nunca hay un listener "colgado" esperando a un proceso que ya no existe, porque el listener vive dentro del mismo proceso `yunta run` que ejecuta el nodo.

**Sin la capacidad, ningún endpoint.** Si el runner resuelto no declara `run_tools`, `SessionRequest.run_tools_endpoint` es `None` y no se levanta nada. Qué pasa cuando el workflow *necesita* la capacidad y el runner no la tiene ya está resuelto en la Spec del Adapter §5 (tabla de degradación): error en `check`, no sorpresa en runtime.
# 7. Verificación automática: checks, baseline y coverage

## 7.1 Nodos `check`

**`gate` y `check` no son variantes de lo mismo**: un `gate` espera a una **persona** — emite `gate_waiting`, el run queda en `waiting` el tiempo que haga falta, alguien decide. Un `check` no espera a nadie: el engine evalúa con datos propios y el nodo sigue o falla, en segundos. Por eso el vocabulario los separa — **gate = espera humana, check = verificación automática** — en lugar de un `gate-builtin` que sugiere una espera que nunca ocurre.

```
- id: no-regressions
	kind: check
	builtin: baseline_compare
	invariant: true
```

Los builtin son una **lista cerrada y corta**, porque un check builtin es por definición algo que el engine ya sabe evaluar con datos que ya tiene; lo extensible es un `executor`, que existe para eso:

- **`baseline_compare`** — la suite del baseline no perdió nada (§7).
- **`coverage_gate`** — `coverage.cmd` sobre el umbral declarado en config.
- **`findings_gate`** — sin hallazgos por encima de una severidad dada (§4.1); acepta `max_severity`.

Cualquier otra verificación se expresa con un nodo `bash` (exit code) o un `executor`. No se agrega un builtin de presupuesto: los límites ya pausan el run por sí mismos (§8.3), y duplicarlo como check sería redundante.

## 7.2 Baseline y coverage

Al crear el run (después del worktree), el engine ejecuta la suite declarada en config (`baseline.suite`), persiste resultados y hash (`baseline_captured`). `baseline_compare` — como nodo `check` o en el cierre implícito — re-ejecuta y falla si algo que pasaba dejó de pasar. Coverage análogo: `coverage.cmd` + umbral, medido y comparado por el engine. "Cero regresiones" y "coverage ≥ N" son comparaciones de datos, nunca afirmaciones.

Ambos comandos entran en la memoización de §5.4 — son deterministas respecto del árbol y caros: si el árbol no cambió desde la última ejecución de esa suite dentro del run, el resultado se reutiliza y se registra como tal. Un workflow con varios `baseline_compare` no paga la suite varias veces sobre el mismo árbol.

## 7.3 Aislamiento del árbol de trabajo
`isolation` se declara en config (`defaults.isolation`), por workflow o por nodo, y admite dos valores en runs de primer nivel:
- **`worktree`**** (default)**: worktree de git dedicado por run, en árbol paralelo al run.dir (§2). Habilita runs concurrentes sobre el mismo repo y aisla al usuario del trabajo del agente.
- **`none`**: el run opera directo sobre el checkout actual. Legítimo para tres casos — ver los cambios en el editor mientras el agente trabaja, CI que ya corre en un contenedor efímero (donde el worktree es puro overhead), y proyectos cuyo setup de árbol es prohibitivo. Condiciones: el engine **exige árbol limpio** al arrancar (no negociable: sin eso, el scope por diff no distingue el trabajo del agente de los cambios del usuario), no admite runs concurrentes sobre ese repo, y el modo queda registrado en el manifest y en el recibo — un run sin aislamiento ofrece menos garantías de reproducibilidad y eso no se oculta.
El costo dominante de wall-clock en muchos proyectos no son los checks sino la preparación del árbol nuevo (instalación de dependencias, build desde cero). La palanca correcta no es saltear aislamiento sino **compartir cachés de build entre worktrees** — directorio de artefactos común por variable de entorno, dependencias enlazadas, o worktrees reutilizables por proyecto. `yunta init` detecta el ecosistema y propone la configuración correspondiente; la guía de autoría lo documenta.
Los nodos `kind: workflow` admiten además un tercer valor, `inherit` (§12): el sub-run comparte el árbol del padre en lugar de crear el suyo. Solo aplica a sub-runs — un run de primer nivel no tiene de quién heredar.
# 8. Resumibilidad y horizonte largo
## 8.1 Dos niveles de reanudación
**Nivel run**: `yunta resume <run_id>` hace replay del log, reconstruye estados, verifica integridad de run.dir y worktree (hashes de artifacts) y retoma cada nodo `running` huérfano según su política. No hay "estado corrupto": o los eventos alcanzan para derivar un estado, o el run se marca `broken` con diagnóstico. Crash del engine, reinicio de máquina y `Ctrl-C` son el mismo caso.
**Nivel nodo**: cada nodo `prompt`/`loop` declara `on_interrupt: restart_node | resume_session | fail_if_uncertain`. `restart_node` es el default y el modo robusto: como los nodos se rehidratan desde datos (§8.2), reejecutar es siempre seguro **para el contexto que recibe el agente**. `resume_session` reanuda la conversación vía el session_id del log; requiere que el adapter declare la capacidad — si no la tiene, el engine degrada a restart con warning. Los reintentos automáticos usan siempre `restart_node` y quedan como intentos numerados en el log.

**Rehidratación no es lo mismo que seguridad de reejecución.** I8 garantiza que un nodo puede *arrancar* de cero con el contexto correcto — eso dice que restart_node siempre puede *intentarse*, no que su efecto sea inocuo. Un nodo cuyo trabajo tiene consecuencias externas (`git push` + `gh pr create`, notificar un webhook, escribir a un sistema de terceros) puede duplicar o corromper si se reintenta a ciegas tras un crash a mitad de ejecución. Por eso **la exigencia de idempotencia de los hooks (I13) se extiende a todo nodo bajo `restart_node`**: si el efecto de correrlo dos veces no es seguro, o se rediseña el comando para que lo sea (patrón check-then-act: `gh pr create` ya falla sin duplicar si la PR existe; preferir esa clase de comandos), o se declara `fail_if_uncertain`.

**`fail_if_uncertain`** es para el residual que genuinamente no se puede hacer idempotente. Si al reanudar el engine encuentra un nodo `running` sin evento terminal (`node_finished`/`node_failed`) en su último intento, en vez de reintentar a ciegas pausa con gate de escalación: *"node X interrupted mid-execution; outcome of the last attempt is unknown — verify manually before retrying."* Nunca asume, nunca reintenta con un efecto potencialmente ya aplicado.
## 8.2 Rehidratación
Un nodo nunca asume historia. Su contexto al arrancar es exactamente: (1) su prompt renderizado, (2) sus fuentes de contexto resueltas (§9), (3) `progress.md`, y (4) sus skills. Nada más. Esto hace equivalentes "primera ejecución", "iteración 7 del loop" y "resume tras tres días": todas nacen igual.
`progress.md` lo genera el engine — no un agente — tras cada `node_finished`, derivándolo del log: qué corrió, qué produjo cada nodo (con la descripción de una línea declarada en el workflow), qué falló, qué sigue. Al ser mecánico, no acumula deriva narrativa.
## 8.3 Presupuestos y cierre
`max_tokens` por nodo y `max_iterations` por loop son contrato: al excederse, el engine emite `run_paused` (razón: límite) y espera decisión humana — nunca degrada en silencio. Al cierre, `on_finish.distill` destila el conocimiento durable (ADRs, [CONTEXT.md](http://CONTEXT.md) bajo `.yunta/knowledge/`) **antes** de cualquier cleanup, y el engine exporta los eventos del run como `events.jsonl` dentro del run.dir: el run archivado queda autocontenido — historia completa más artifacts en un directorio copiable, con vida independiente de la retención. El event log en base se conserva según `storage.retention_days` aunque run.dir se borre. Las specs de un run son efímeras; sus decisiones no.
## 8.4 Contabilidad de costos: costo por tarea verificada
Los `Usage` de adapter se atribuyen al nodo — y, dentro del ciclo del ledger, a la tarea — que los generó. Sobre esa atribución el engine deriva del log, sin estimaciones: **CPTV (costo por tarea verificada)** = tokens totales del run / tareas `done`; **tasa de re-trabajo** = tokens gastados en reintentos y re-rutas / totales; **tasa de cache** = tokens leídos de cache / input totales (si el adapter los distingue en `Usage`, extensión opcional de `usage_reporting`); y costo por nodo, por rol y por modo. `yunta stats <run_id>` las muestra; `yunta stats --workflow X` agrega histórico para comparar modos, runners y versiones del workflow con datos. La métrica de cabecera es CPTV porque optimiza lo que importa: no minimizar tokens — un run barato que no verifica nada es carísimo — sino el costo de cada unidad de trabajo demostrada.

**CPTV es en tokens, siempre; la moneda es una conveniencia opcional y nunca la fuente de verdad** — el precio por token cambia con el proveedor y el modelo, y el engine no debe saber de pricing para funcionar. Si `pricing:` está declarado en config (`{model: cost_per_1k_tokens}`), `stats` y el recibo agregan una línea de estimado en moneda **junto a** los tokens, nunca en su lugar; sin `pricing:` declarado, todo se expresa en tokens y nada se inventa.
## 8.5 Progreso observable
El progreso es un dato derivado del log — nunca una estimación ni un reporte de agente. Dos niveles, porque miden cosas distintas: **flujo** (nodos terminados sobre el DAG congelado en el manifest, con `waiting` distinguido: un run esperando un gate no está estancado, está esperando a una persona) y **tarea** (tareas `done`/total del ledger — la medida honesta durante los nodos largos de implementación, donde el nivel flujo se quedaría quieto por horas).
Presentación normativa: **contadores con contexto, no porcentajes** — `14/23 tasks · 3/9 nodes · 2 reroutes · waiting on gate approve-plan`. Los porcentajes mienten en cuanto hay re-rutas (el denominador crece), loops sin cota fija o promociones (el sucesor resetea). El progreso nunca cambia en silencio: si una re-ruta o un re-plan agranda el denominador, el cambio es visible y atribuible a su evento.
Superficies: `yunta status <run>` (snapshot), `yunta run --follow` (en vivo, consumiendo el stream de eventos) y la tool MCP `workflow_status` (estructurado, para que el agente cliente lo comunique). Composición: el padre presenta el progreso de los hijos **como árbol, jamás promediado en un número único** — promediar hijos heterogéneos es otra forma de porcentaje mentiroso.
## 8.6 Estimación previa

El histórico responde qué costó; la misma data responde qué va a costar. Antes de arrancar, el engine deriva de los runs pasados del mismo workflow la distribución observada — mediana y p90 de tokens, wall-clock, tareas — y la muestra: *"12 past runs · median 340k tokens, p90 520k · median wall-clock 22 min"*. No es una predicción: es lo que ya pasó, que es la única estimación honesta que un sistema puede ofrecer.

Es **informativa, nunca bloqueante**: se muestra en `yunta run` al crear el run y viaja en `list_workflows`, donde puede cambiar qué workflow elige un agente cliente. Lo que sí es accionable: cuando el presupuesto declarado queda por debajo del p90 histórico, el engine advierte antes de gastar — *"budget 200k is below the p90 of past runs (520k); this run will likely pause"* — porque un run que se detiene a mitad por un límite mal elegido es el desperdicio más caro que hay.

Sin datos suficientes (menos de tres runs del mismo workflow), el engine **no dice nada**. Un número sin distribución detrás es adivinanza con apariencia de dato; tampoco se estima de forma estática contando nodos y multiplicando por un promedio genérico, por la misma razón por la que el progreso se expresa en contadores y no en porcentajes (§8.5).

## 8.7 Rendimiento de la verificación

El mismo criterio que el engine aplica al trabajo se aplica a la ceremonia del propio workflow: **si algo no puede fallar, no está probando nada**. Sobre el histórico, el engine detecta verificación que dejó de rendir y lo informa.

La métrica núcleo es la **tasa de rojo en pre-check**, no la de fallos totales — la distinción es esencial y confundirla haría que el sistema sugiriera borrar los criterios que mejor funcionan. Un criterio que **nunca estuvo en rojo antes del trabajo** es sospechoso: no prueba que el trabajo se hizo. Un criterio que está rojo antes y verde después, siempre, está funcionando exactamente como debe.

Hallazgos y sus lecturas, cada uno acompañado del conteo que lo sostiene — sin el dato crudo, una sugerencia es una opinión:

| Señal | Lecturas posibles |
|---|---|
| Criterio nunca en rojo en pre-check | es redundante, **o** está mal escrito — ambas se muestran, nunca una sola |
| Re-ruta que nunca se disparó | el flujo previo es más confiable de lo previsto |
| Gate siempre aprobado sin ajuste | ¿sigue agregando valor o quedó como ritual? |
| Modo que nadie elige | candidato a eliminarse |
| Tareas que siempre pasan al primer intento | el plan puede estar cortando demasiado fino |

**Superficies.** Además de `yunta stats --workflow`, los hallazgos aparecen en `yunta check` — es decir, en el momento en que alguien ya está tocando ese workflow, cuando la observación es accionable. Un reporte que solo vive en un comando de análisis se consulta una vez y se olvida.

**Tres guardas, no negociables.** (a) **Sugiere, jamás actúa**: el engine no puede quitarse verificación a sí mismo — eso vaciaría de sentido al recibo; propone, decide una persona, y el cambio queda como edición del workflow con su commit. (b) **Nunca sugiere quitar nodos `invariant: true`**: su valor no se mide en frecuencia de fallo — un baseline que nunca falla es un baseline que está funcionando. (c) **Evidencia suficiente por criterio**, no por workflow: con pocas observaciones el engine no dice nada, y un criterio nuevo en un workflow viejo no hereda historia.

## 8.8 Exportación de telemetría (OpenTelemetry, post-v1)

La clave de config `telemetry:` está retirada del schema hasta que este exportador exista (D121): una clave que el engine solo parsea y nunca aplica es una degradación silenciosa, así que vuelve junto con el exportador y su propio ADR de activación. Lo que sigue es el diseño que esa clave gobernará cuando llegue.

Cuando se active, el engine exporta cada run como **trace** y cada nodo como **span**, generados por **replay del event log** — nunca una fuente de verdad paralela, así que un run viejo puede exportarse retroactivamente si la telemetría se activa después, y jamás hay dos historias del mismo run.

Atributos de trace (`service.name: "yunta"`): `yunta.run_id`, `yunta.workflow`, `yunta.workflow_version`, `yunta.mode`, `yunta.manifest_hash`, `yunta.outcome`, `yunta.tokens_total`, `yunta.cptv`. Atributos de span, uno por nodo, proyectados uno a uno desde el event log: `yunta.node_id`, `yunta.node_kind`, `yunta.runner_role`, `yunta.runner_resolved` (adapter+model), `yunta.outcome`, `yunta.retries`, `yunta.tokens_input/output/cached`.

Nombres de atributo con prefijo `yunta.` — sin adoptar todavía las convenciones semánticas `gen_ai.*` de OTel, que siguen en movimiento; migrar es un mapeo de nombres cuando maduren, no un rediseño. **Los spans heredan las mismas reglas de redacción que los eventos (I12)**: son metadata — dónde se fue el tiempo y el costo — nunca contenido; ningún atributo lleva prompt, output o path con datos, ni siquiera resumido.

Config (`telemetry:`): `enabled`, `endpoint` (OTLP, default un collector local), `protocol` (`grpc | http`).

# 9. Inyección de contexto
Trait del engine:
```
trait ContextSource \{
	fn id(&self) -> &str;
	async fn resolve(&self, ctx: &RunCtx) -> Result<ResolvedContext>;
\}
// ResolvedContext = archivos bajo context/<hash>/ + modo de montaje
```
Builtin: `files` (globs del repo o run.dir), `command` (stdout con timeout), `artifact` (output de un nodo previo — crea dependencia implícita en el DAG), `mcp` (query a un server MCP externo), `run-events` (consulta de solo lectura al log), `ledger` (la tarea propia, o el estado agregado para nodos de auditoría), `knowledge` (conocimiento durable, ver §9.2) y `node-output` (stdout/stderr capturado de un nodo, ver §11.2). Los equipos agregan fuentes propias como executors, sin tocar el core.
Reglas: las fuentes se resuelven **antes** de abrir la sesión; el resultado se materializa en `context/` y se monta según volumen (inline si es chico, como archivos referenciados si es grande — umbral configurable); cada resolución emite evento con hash, así el replay sabe exactamente qué vio cada nodo. Una fuente que falla es fallo del nodo, no contexto silenciosamente ausente.

**El hash identifica, no sustituye.** Para que un nodo sea *replayable* de verdad — poder responder "¿qué vio exactamente el agente acá?" (RFC-0003 §2) — el **contenido efectivo** de cada fuente que participó tiene que estar materializado bajo `context/<hash>/`, no solo su hash. Esto ya es lo que el trait implica (`ResolvedContext = archivos bajo context/<hash>/`); se hace explícito para que ninguna fuente builtin lo incumpla por comodidad: `files` guarda el snapshot leído; `command` guarda stdout/stderr y exit code, no solo que corrió; `knowledge` guarda el contenido resuelto de las capas, no una referencia a `.yunta/knowledge/`; `mcp` guarda el request y la respuesta recibida, no solo la query; `run-events`/`ledger`/`node-output` guardan el fragmento del log efectivamente leído. La prueba de que esto se cumple es operacional: **reconstruir el contexto de cualquier sesión pasada nunca debe requerir volver a llamar al servicio, comando o archivo original** — si lo requiere, esa fuente no es replayable y `replay` (post-v1, RFC-0003 §2) no puede prometer nada sobre ella.
Sintaxis por nodo — cada entrada de `context:` es una fuente con sus parámetros:
```
- id: plan
	context:
		- files: \["docs/[architecture.md](http://architecture.md)", "\{\{run.dir\}\}/artifacts/[brief.md](http://brief.md)"\]
		- command: "git log --oneline -20"           # stdout → contexto, con timeout
		- mcp: \{ server: internal-docs, query: "\{\{inputs.idea\}\}" \}  # server declarado en config (mcp_servers)
		- artifact: \{ node: grill, name: [brief.md](http://brief.md) \}   # crea dependencia implícita grill → plan
		- ledger: \{\}                                  # la tarea propia (executors) o estado agregado (auditoría)
		- knowledge: \{\}                               # conocimiento durable en capas (§9.2)
		- node-output: \{ node: lint \}                 # stdout/stderr capturado de un nodo
		- run-events: \{ filter: failed \}              # consulta de solo lectura al log
```
Demarcación normativa: `context:` inyecta **datos** (sobre qué trabajar); `skills:` monta **instrucciones y capacidades** (cómo trabajar) por el mecanismo nativo del adapter. Son propiedades separadas del nodo deliberadamente — un skill no es una fuente de contexto y no pasa por `ContextSource`.
## 9.1 Ensamblado estable-primero
El costo por token de la rehidratación (§8.2) depende de que el proveedor pueda reutilizar prefijos ya procesados (prompt caching). El engine no gestiona el cache — es del proveedor/CLI — pero garantiza la condición que lo habilita: **prefijos byte-estables entre sesiones**. Cada fuente tiene una clase de estabilidad, declarada o inferida por el engine: `stable` (skills, knowledge, archivos del repo que el run no toca), `run-stable` (artifacts congelados: brief, plan, lo derivado del manifest) y `volatile` (node-output, run-events, command, [progress.md](http://progress.md), la tarea del ledger). El engine ensambla el contexto SIEMPRE en ese orden — estable → run-estable → volátil → prompt del nodo — con serialización canónica: mismo orden de fuentes, mismos separadores, sin timestamps ni contenido no determinista dentro de los segmentos estables. Así, la iteración 7 de un loop y el resume de mañana comparten prefijo byte-idéntico con la iteración 1, y el cache del proveedor hace el resto. El evento `context_assembled` registra los hashes por segmento: comparar hashes entre sesiones es la verificación mecánica de que el prefijo se mantuvo estable — y el diagnóstico exacto cuando no.
## 9.2 Knowledge: capas y alcance multi-proyecto
El conocimiento durable que `distill` produce (§8.3) se consume por la fuente `knowledge`, que resuelve **en capas, igual que config y skills**: `repo` (`.yunta/knowledge/`, lo destilado acá) > `user` (`~/.yunta/knowledge/`) > `org`. Precedencia local: lo del repo pisa a lo general ante conflicto.
```
context:
	- knowledge: \{\}                     # todas las capas disponibles
	- knowledge: \{ layers: \[repo\] \}     # solo local — para nodos que no deben contaminarse
```
La **capa org es un pack** (RFC-0002 íntegro, sin mecanismo nuevo): ADRs transversales y convenciones curadas se publican como `<publisher>/org-knowledge@vN`, se vendorean con lockfile y quedan congelados por run. Actualizar conocimiento compartido es una decisión explícita y versionada, jamás una sincronización automática.
La promoción de conocimiento local a org **no es automática y no la hace el engine**: es un workflow de Yunta como cualquier otro (candidatos desde los `knowledge/` de los repos → gate con `assignee` curador → nueva versión del pack). Para volúmenes que exceden lo que conviene montar como archivos, la válvula es la fuente `mcp` apuntando a un RAG interno — explícita en el workflow, nunca una capa implícita.
## 9.3 Prompts: inline o desde archivo
`prompt` acepta dos formas, sin claves adicionales:
```
prompt: "Implementá la siguiente tarea del ledger"   # inline
prompt: \{ file: prompts/[plan.md](http://plan.md) \}                    # desde archivo
```
Un valor escalar es el prompt; un mapa declara de dónde sale — el mismo idioma que usan las fuentes de contexto para expresar procedencia. No hay heurística de "si parece una ruta": la forma del valor lo dice, de modo que un prompt de una línea que casualmente se parezca a un path nunca se interpreta como archivo. La extensión futura (otras procedencias, composición por partes) es un campo más en ese mapa, no una clave nueva por caso.
La ruta se resuelve relativa al workflow que la declara; el contenido se renderiza con los mismos templates que el inline y **entra al hash del manifest igual que él**: editar el archivo a mitad de run no altera ese run (I3). `yunta check` valida existencia y no-vacuidad antes del primer token. Prompts largos en archivo son además contenido `stable` para el ensamblado de §9.1, y quedan legibles en un diff de PR y en el inventario de `pack audit` (RFC-0002 §6) — revisables como archivos, no como bloques YAML.
# 10. Modos y promoción
## 10.1 Modos
Un workflow puede declarar variantes de modo. Los nombres y la cantidad son **libres y del autor del workflow**: `modes:` es un mapa ordenado abierto, no un conjunto fijo del schema — quick/standard/full son convención de los workflows de referencia, no palabras reservadas:
```
modes:
	hotfix:   \{ include: \[implement, lint, tests, ship\] \}
	standard: \{ include: \[grill, plan, implement, lint, tests, review, ship\] \}
	audit:    \{ include: all \}
```
El **orden de declaración define la escalera**: la promoción (§10.2) va de un modo a cualquiera posterior en la declaración. Reglas verificadas por `yunta check`, invariantes al nombre y al número de modos: los nodos `invariant: true` (verificación, scope, baseline, higiene) presentes en **todas** las variantes — un modo recorta deliberación, jamás verificación —; todo modo referencia nodos existentes; y la clasificación la propone un nodo temprano **entre los modos declarados** y la confirma un gate, quedando congelada en el manifest.
**Dependencias sobre nodos excluidos.** Un nodo incluido que declara `depends_on` hacia un nodo que el modo excluye espera lo que ese nodo esperaba: hereda sus dependencias incluidas, transitivamente. En un modo sin `approve-plan`, `implement` espera a `plan`; en un modo sin `review` ni `fix-findings`, `ship` espera a `tests`. Un modo recorta deliberación, nunca el orden del trabajo que queda; una única derivación pura produce el grafo de cada modo para el scheduler, la escalación y `status`.
**Coherencia interna del modo.** Cada variante se valida como si fuera un workflow completo: si un modo incluye un nodo cuyo `on_failure.goto` — o cuya opción de gate — apunta a un nodo excluido de esa variante, es **error de ****`check`****, no warning**. Que el destino exista en el archivo pero no en el modo que se va a correr es la misma referencia rota que un `goto` hacia un nodo inexistente, y dejarlo pasar significa que el flujo revienta recién cuando el lint falla: después de gastar tokens, por algo detectable antes de arrancar. El mensaje del error nombra las dos salidas posibles — incluir el destino en el modo, o quitar la re-ruta en esa variante — porque un error de validación que no dice cómo salir es la mitad de un error.
## 10.2 Promoción = run sucesor
Cuando el trabajo revela que el problema excede el modo (scope insuficiente, hallazgo estructural, decisión de diseño inesperada), un nodo o el engine emiten `promotion_signaled` y el run pausa en un gate. Si el humano acepta, el engine cierra el run (`run_finished: promoted`) y crea uno nuevo en un modo **posterior en el orden de declaración** (§10.1) con `promoted_from: <run_id>`, cuyo contexto inicial incluye automáticamente artifacts, ledger y findings del antecesor (fuente `artifact` cross-run, permitida solo a través de `promoted_from`). La cadena queda auditada en ambos logs. Retroceder a un modo anterior en la declaración no existe: no hay mecanismo que lo exprese.
# 11. Ciclo de vida de nodo: hooks y re-rutas
## 11.1 Hooks `before` / `after`
Todo nodo puede declarar comandos determinísticos alrededor de la sesión — pegamento que no amerita un nodo propio (instalar dependencias, levantar servicios, formatear, limpiar temporales). Van agrupados bajo `hooks:`, porque `before` y `after` solo tienen sentido juntos y comparten las mismas reglas:
```
- id: implement
	hooks:
		before:
			- run: "npm ci --prefer-offline"
		after:
			- run: "cargo fmt"
			- run: "rm -rf .tmp-fixtures"
				on_failure: warn            # fail (default) \| warn
```
Secuencia del nodo: resolver contexto → `hooks.before` → sesión del agente → `hooks.after` → verificación (criterios post, scope, artifacts). Consecuencias deliberadas del orden: un `before` que falla aborta el nodo sin gastar tokens, y el `after` corre antes de la verificación, de modo que los criterios evalúan el estado final real. Las ediciones de los hooks cuentan dentro del diff del nodo: el scope también las gobierna — un hook no es una puerta trasera al drift.
Reglas: solo comandos, nunca IA (para eso existen los nodos); idempotencia obligatoria — los hooks re-corren en cada reintento y cada resume; timeout corto configurable; cada ejecución emite `hook_executed`. `node_defaults.hooks` a nivel workflow evita repetición. Prohibido inyectar hooks desde capas de configuración que no sean visibles en el workflow que el equipo lee: si una organización quiere imponer hooks, lo hace vía workflows compartidos, donde se ven. Criterio de demarcación: si el comando tiene lógica de negocio o duración significativa, es un nodo `bash` con estado y visibilidad propios, no un hook.
## 11.2 Re-rutas por fallo (`on_failure.goto`)
El grafo de `depends_on` es acíclico y lo sigue siendo. Las **aristas de re-ruta** son un segundo conjunto de aristas, separado, que expresa ciclos controlados de corrección:
```
- id: lint
	kind: bash
	depends_on: \[implement\]
	run: "npm run lint"
	on_failure: \{ goto: fix-lint, max_reroutes: 2 \}
```
Semántica: al fallar el nodo, el engine emite `node_rerouted` y transfiere control al destino (que puede ser un nodo fuera del camino principal, existente solo para esto). Cuando el destino y su subgrafo completan, **el nodo fallido vuelve a ****`ready`**** y re-corre**. El contador `max_reroutes` es por nodo fallido; al agotarse, `run_paused` + gate de escalación en formato cuestionario — el ciclo jamás es infinito ni silencioso. `yunta check` valida que todo `goto` apunte a un nodo existente y que el subgrafo de corrección no dependa del nodo fallido.
El output del nodo fallido (stdout/stderr, acotado) se captura como artifact automático, y el nodo de corrección lo monta con la fuente builtin `node-output`:
```
- id: fix-lint
	kind: prompt
	context:
		- node-output: \{ node: lint \}
	prompt: "Corregí exclusivamente los errores del reporte de lint."
	scope: \["src/"\]
```
Escalera de corrección — usar el peldaño más bajo que alcance: (1) fix mecánico → hook `after` (p. ej. `lint --fix`), sin IA; (2) fallo de una tarea puntual → los `criteria` del ledger lo rebotan dentro del ciclo de tareas; (3) validación transversal al final del flujo → re-ruta con nodo correctivo.
# 12. Composición: workflows como nodos y runs vinculados
Un nodo `kind: workflow` ejecuta otro workflow como sub-run:
```
- id: qa
	kind: workflow
	use: qa-review
	inputs: \{ branch: "\{\{run.branch\}\}" \}
	isolation: inherit            # worktree (default) \| inherit
```
**Cada sub-workflow es un run completo**, con run_id, manifest, event log y run.dir propios — nunca una expansión inline. El padre emite `child_run_created` (con `workflow_hash` del hijo, además del `child run_id`, para que la identidad efectiva quede en el evento sin ir a buscar el manifest), espera el estado terminal del hijo (`child_run_finished`) y trata ese resultado como el resultado del nodo. Consecuencias: cada pieza conserva resumibilidad, auditoría y presupuestos propios; `yunta resume` del padre retoma hijos huérfanos recursivamente; y un proceso de semanas es un run padre que pasa la mayor parte de su vida en `waiting` sin ningún proceso corriendo.

**Reproducibilidad histórica, explícita.** El padre congela **nombres e inputs** del hijo (más abajo), nunca su manifest — pero eso no deja un hueco de reproducibilidad, porque **el `child_run_id` es la referencia inmutable y el manifest congelado del hijo es su propia fuente histórica de verdad** (I3 ya lo garantiza para *cualquier* run, el hijo incluido). Reproducir un run padre de hace seis meses nunca vuelve a resolver `nombre-del-workflow@versión-actual`: sigue el `child_run_id` grabado en `child_run_created` hasta el manifest de ese run específico, que está congelado desde que nació y no le importa que el workflow del hijo haya cambiado después. No hace falta duplicar el manifest del hijo dentro del padre — alcanza con la referencia, porque el hijo ya es inmutable por sí mismo.

**Runs vinculados.** Los vínculos declarados (`parent/child`, `promoted_from`) forman un grafo auditado. La fuente de contexto `artifact` cross-run opera exclusivamente a través de vínculos: un hijo puede montar artifacts del padre o de hermanos terminados; nadie monta artifacts de runs ajenos. La promoción (§10.2) es un caso particular de este mecanismo general.
**Manifests.** El padre congela los **nombres e inputs** de sus hijos, no sus manifests: cada hijo resuelve y congela su propio workflow al nacer. Un proceso largo incorpora mejoras a los workflows hijos entre ejecuciones, sin violar la inmutabilidad de ningún run individual.
**Aislamiento.** `worktree` (default) da a cada hijo su árbol; `inherit` comparte el del padre para fases de una misma pieza de trabajo — hijos paralelos con `inherit` exigen scopes disjuntos, validado en check.
**Personas.** Los gates llevan `assignee` (rol o identidad): `gate_waiting` notifica a esa audiencia por los canales de config y `gate_resolved` registra quién resolvió. La coordinación entre personas es estado del run, no convención externa.
**Presupuestos en cascada.** El `Usage` de los hijos agrega hacia arriba; los límites del padre pueden pausar el árbol entero.
**Límites deliberados.** `yunta check` valida que el grafo de referencias entre workflows sea acíclico y respete una profundidad máxima configurable. Los triggers asincrónicos entre workflows sin padre común ("cuando termine X, disparar Y") quedan fuera del engine: son territorio del CI del equipo o del proyecto de servidor separado.
# 13. Runners: roles, resolución y agentes del adapter
Terminología del contrato, tres niveles que no se mezclan: un **adapter** es la integración con un CLI (claude-code, codex, mock); un **runner** es un binding concreto `{adapter, model, agent?, permissions, env}` definido en config, que es lo que los nodos piden; un **agente** es un agente nombrado *del* adapter (los definidos por el equipo dentro de su CLI). El campo `agent:` de un runner es **portable**: cada adapter lo traduce a su mecanismo nativo de agentes y declara la capacidad `custom_agents`; un runner que pide `agent:` sobre un adapter sin la capacidad es error de `yunta check`. No existen claves propietarias por adapter para esto — lo específico de un adapter que no tenga expresión portable va en `adapter_settings`, y la selección de agente no califica.
Los workflows declaran **roles**, no runners concretos: `runner: planner`, `runner: executor`. La config en capas resuelve cada rol; nombres concretos siguen permitidos pero desaconsejados en workflows compartidos — el rol es lo que hace que el mismo workflow corra en equipos con herramientas distintas.
Nota terminológica normativa: "rol" es vocabulario descriptivo de esta spec y **nunca una clave del schema** — el keyword es `runner:` en nodos y `runners:` en config, y el mismo campo acepta un nombre resoluble por config (canónico) o una referencia concreta. No existe `role:` como clave, deliberadamente: la palabra queda libre para los roles humanos de los gates (`assignee`), que son otra cosa y no deben colisionar.
## 13.1 Candidatos ordenados
Un rol puede resolver a una **lista ordenada de candidatos**, mezclando adapters, modelos y agentes libremente:
```
runners:
	planner:
		- \{ adapter: claude-code, model: claude-opus-4-8 \}
		- \{ adapter: codex, model: gpt-5-codex \}              # fallback
	reviewer:
		- \{ adapter: claude-code, model: claude-sonnet-4-6, agent: benito \}
		- \{ adapter: codex, model: gpt-5-codex \}
```
La resolución ocurre **una vez, al crear el run**: el engine recorre los candidatos en orden y elige el primero cuyo `probe()` pasa (incluida la existencia del agente pedido) y cuyas capacidades satisfacen todos los nodos que usan el rol. La elección — y cada candidato descartado con su causa — se registra (`runner_resolved`) y queda congelada en el manifest: un run no cambia de runner a mitad de camino, y un resume reutiliza la resolución. `yunta check` valida estáticamente que todo rol usado esté definido y que al menos un candidato satisfaga lo exigido.
Restricción deliberada: la resolución es por run, jamás por nodo en runtime. "Probá con un modelo barato y si falla escalá a uno caro" no es fallback de disponibilidad sino escalación de calidad, y se expresa con re-rutas `on_failure.goto` hacia un nodo equivalente con rol más caro — visible en el DAG en vez de escondido en la resolución.
## 13.2 Fan-out multi-runner
Para los casos donde el valor está en la diversidad — reviews, segundas opiniones — un nodo puede declarar varios roles y el engine lo expande en instancias paralelas del mismo nodo, una por rol, en la creación del manifest:
```
- id: review
	kind: prompt
	runners: \[reviewer, reviewer-alt\]     # expande a review@reviewer ∥ review@reviewer-alt
	permissions: read-only
	prompt: "Auditá los cambios; hallazgos a \{\{run.dir\}\}/artifacts/findings-\{\{runner.role\}\}.yaml"
	artifacts:
		produces: \[\{ name: "findings-\{\{runner.role\}\}.yaml", kind: findings \}\]
```
Cada instancia es un nodo pleno (eventos, artifacts, presupuesto propios); la consolidación de hallazgos es un nodo posterior normal. La expansión es estática — el DAG resultante queda en el manifest, no hay dinamismo en runtime.
## 13.3 Agente a nivel nodo
Un nodo puede declarar `agent:` directamente, y **prevalece sobre el ****`agent`**** del runner resuelto**:
```
- id: review-security
	kind: prompt
	runner: reviewer
	agent: security-auditor    # override del agente del binding para este nodo
```
La resolución de candidatos (§13.1) lo incorpora: un candidato solo satisface un rol si su adapter declara `custom_agents` y `probe()` verifica la existencia de **todos** los agentes pedidos — los de los candidatos y los declarados a nivel nodo por quienes usan ese rol. Consecuencia de portabilidad: un `agent:` a nivel nodo nombra un agente de un adapter concreto y por lo tanto restringe qué candidatos pueden resolver el rol. Guía: en workflows compartidos entre equipos, preferir el agente en los candidatos del runner (donde cada adapter empareja su propio equivalente); reservar el override por nodo para workflows internos. El runner conserva su campo `agent:` precisamente por eso — es lo que permite que candidatos de adapters distintos aporten cada uno su agente nombrado.
# 14. Tests de workflow

Un workflow es código: tiene re-rutas, caps, modos y condiciones que se rompen como cualquier lógica. El engine verifica el trabajo de los agentes; los workflows también necesitan verificarse, y con el mismo criterio — mecánicamente, sin intervención de un LLM.

Un caso de test declara qué correr, con qué guión de mock, y qué debe haber ocurrido:

```
# .yunta/tests/lint-recovery.yaml
workflow: build-feature
mode: quick
inputs: \{ idea: "test" \}
fixture: fixtures/lint-fails-twice.yaml   # guión de eventos y efectos del adapter mock
expect:
	final_state: finished
	nodes:
		lint: \{ reroutes: 2 \}
		fix-lint: \{ runs: 2 \}
	tasks:
		T001: done
	events:
		- node_rerouted: \{ count: 2 \}
	never:
		- task_status_changed: \{ task: T002, to: done \}
```

`yunta test` ejecuta cada caso con el adapter `mock`, deriva el estado por replay y lo compara contra `expect`. Sin LLM, sin red, determinístico y rápido — apto para correr en cada commit. No hay maquinaria nueva: el mock con fixtures, la derivación por replay y el estado del run ya existen; `test` es un comparador sobre datos que el engine ya produce.

Las aserciones cubren **estado final y secuencia**: el estado final es lo que importa, pero poder afirmar sobre eventos — cuántas re-rutas hubo, qué nunca ocurrió — es lo que atrapa las regresiones sutiles, que en un workflow suelen ser de camino y no de resultado.

Los casos viven en `.yunta/tests/`, versionados junto a los workflows que prueban. **Un pack puede traer los suyos** (RFC-0002): `pack add` no los ejecuta por default, pero `pack audit` reporta si el pack incluye tests y si pasan — un pack testeado es una señal de calidad verificable, no una promesa del autor.

# 15. Invariantes (resumen normativo)
I1. Toda información necesaria para continuar un run está en el event log o en run.dir.
I2. El event log es append-only; el estado siempre es derivable por replay. Eventos y payloads versionados.
I3. El manifest es inmutable; cambiar de reglas o de modo = run sucesor con `promoted_from`.
I4. Los artifacts son inmutables; el progreso son eventos, no ediciones.
I5. Una tarea está `done` solo por decisión del engine tras criterios verdes y scope limpio; ningún agente tiene vía para marcar estado.
I6. Todo criterio no-guard debe estar en rojo antes del trabajo; un criterio que ya pasa rebota la tarea a plan.
I7. Condiciones de loop, presupuestos, baseline y coverage los evalúa el engine sobre datos propios.
I8. Todo nodo debe poder ejecutarse desde una sesión recién nacida.
I9. Las capacidades de adapter (resume_session, edit_hooks) se declaran; el engine degrada explícitamente, nunca en silencio.
I10. Los modos recortan deliberación, jamás nodos `invariant: true`.
I11. Ninguna degradación es silenciosa: límites, fuentes caídas, drift de scope y señales de promoción pausan o fallan con diagnóstico. Un **diagnóstico** es un valor y no una frase (D130): nombra su sujeto en el vocabulario del documento, lleva un código estable por clase de problema, viaja en el event log con esa forma, y se redacta una vez en el borde para el lector que lo va a leer — una persona en la terminal, un agente que va a reintentar.
I12. El event log jamás contiene secretos; los agentes los reciben solo vía env vars declaradas en el manifest y el engine los redacta de todo payload.
I13. Los hooks son determinísticos e idempotentes; sus ediciones quedan bajo el mismo scope que el nodo; ninguna capa invisible al workflow puede inyectarlos.
I14. Toda re-ruta tiene cap y su agotamiento pausa con escalación; las aristas de re-ruta son un conjunto separado que nunca relaja la aciclicidad de `depends_on`.
I15. Un sub-workflow es un run completo con manifest propio; el padre congela referencias e inputs, jamás manifests ajenos. Los artifacts cruzan runs solo vía vínculos declarados.
I16. Los recursos agregan hacia arriba en el árbol de runs y los límites del padre lo gobiernan; el grafo de referencias entre workflows es acíclico y de profundidad acotada.
I17. La resolución rol→runner ocurre al crear el run (probe + capacidades sobre candidatos ordenados), queda congelada en el manifest y todo fallback se registra con causa. La selección de agente del adapter es el campo portable `agent:`, nunca un setting propietario.
I18. `permissions` es un solo modelo de techos: cada nivel (org → repo/usuario → pack → nodo → scope) solo puede estrechar al anterior, y se hace cumplir en check y en runtime pre-ejecución.
I19. El contexto se ensambla siempre estable → run-estable → volátil → prompt, con serialización canónica y hashes por segmento registrados; ningún contenido no determinista entra en un segmento estable.
I20. Progreso, costos y recibos son derivaciones del log — nunca estimaciones ni texto redactado por un agente.
I21. Ningún agente amplía su propio scope: lo solicita con criterio verificable y lo concede el engine (por regla o por persona), con cap por run y traza en log y recibo.
I22. Los criterios son deterministas respecto del árbol de trabajo y por eso memoizables; lo no determinista pertenece a nodos, que nunca se memoizan.
I23. La rehidratación (I8) no implica seguridad de reejecución: todo nodo bajo `restart_node` debe ser idempotente en sus efectos externos, igual que los hooks (I13); el residual no-idempotente declara `fail_if_uncertain`, que escala en vez de reintentar a ciegas ante un intento de resultado desconocido.
I24. Una colisión de escritura entre hijos de un `parallel` es error de `check` solo si ambos declaran scope y se solapan; sin scope declarado, el sistema no puede detectarla y lo dice explícitamente vía warning — nunca finge una garantía que no puede cumplir.
I25. Ninguna tool de control MCP bloquea por la duración de un run; la vida de un run es independiente de la sesión MCP que lo creó.
I26. Todo evento persiste un `event_hash` encadenado sobre los bytes exactamente como se escribieron, antes de normalización; una cadena rota marca el run `broken` con diagnóstico. La cadena da integridad y orden, nunca autoría — eso es firma (A-06), deliberadamente separada.
I27. Cada endpoint MCP por-run está scopeado por construcción a `(run_id, node_id, intento)` en el token que lo autentica, nace con la sesión del nodo y muere con ella; ninguna tool acepta un identificador de run como parámetro del llamador.
I28. Un `child_run_id` es la referencia histórica inmutable de una composición; reproducir un run padre nunca vuelve a resolver la versión actual de un workflow hijo, siempre sigue al manifest congelado del hijo que efectivamente corrió.
I29. `permissions.network` es declarativa: el engine no provee ni promete aislamiento de red a nivel de sistema operativo: eso, si existe, es responsabilidad de un executor o del entorno de ejecución.
I30. Toda fuente de contexto que participó en una sesión queda con su contenido efectivo materializado, no solo su hash; reconstruir el contexto de una sesión pasada nunca requiere volver a consultar el origen.
```
