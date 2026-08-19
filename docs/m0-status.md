# M-0 — Estado de implementación

## ✅ Criterio de éxito de M-0 — cumplido

> "La primera tarea del plan implementada por Yunta sobre sí mismo, con criterios
> en verde y scope limpio." (Plan de implementación, sección M-0)

Ejecutado el 2026-08-19 con **T7.8 `yunta graph`** (candidata sugerida por el propio
Plan), recortado al schema de M-0 (Mermaid únicamente, `depends_on` + `on_failure.goto`
diferenciados, `--run <id>` con estado derivado — sin `parallel`/`gate`/composición,
que no existen en el recorte de T1.1):

1. Escribí a mano el ledger (`plan.yaml`, una tarea `T7008`) y el test de aceptación
   (`crates/cli/tests/graph_cmd.rs`) — el pre-check en rojo lo confirmó: 2 de 3
   sub-tests fallaban porque `graph` ni siquiera era un subcomando reconocido.
2. Corrí `yunta run` de verdad, con el adapter `claude-code` real (modelo
   `claude-sonnet-5`) apuntado a un **worktree aislado** (`git worktree add`, rama
   descartable) — nunca al checkout de esta sesión — para que una sesión real
   implementara la tarea sin que yo escribiera el código.
3. El engine nunca confió en el reporte del agente (I5): re-corrió los criterios él
   mismo. Pasaron los tres — el test de aceptación, `clippy -D warnings` y `fmt
   --check` (ambos `guard`) — y el propio mecanismo de `run_task`/§5.5 hizo el commit
   local automáticamente al completarse la tarea.
4. Revisé el diff a mano (código idiomático, reutiliza `check_or_refuse` existente,
   sigue el patrón de `status.rs`), corrí la suite completa del workspace de forma
   independiente en el worktree (fmt/clippy/tests/aislamiento por crate — todo verde,
   sin regresiones fuera del scope de la tarea) y traje el commit verificado a la
   rama real por cherry-pick (`79c2f39`) + el test de aceptación (`f1a0bed`).

Esto valida la tesis central del bootstrap: el ciclo pre-check en rojo → dispatch →
post-check → commit funciona de punta a punta con un agente real, sin intervención
humana en la implementación. Las tres preguntas que M-0 debía responder (pre-check
practicable, scope por diff sin ruido excesivo, — la tercera, ledgers válidos
generados por un agente, queda para cuando exista el nodo de planificación, ya que acá
el ledger lo escribí yo a mano) tienen ahora evidencia real, no solo con mock.


Documento de seguimiento, no normativo — se actualiza en cada sesión de trabajo para
que retomar el bootstrap no dependa de memoria de conversación. La fuente de verdad
del *qué* sigue siendo el Plan de implementación (Notion, sección M-0); esto es
únicamente el *dónde estamos* y el *qué quedó deliberadamente afuera y por qué*.

## Alcance mínimo de M-0 (tal como lo define el Plan)

- [x] **T1.0** — spec del ledger → `docs/spec-ledger.md`
- [x] **T2.0** — spec de eventos → `docs/eventos.md` (31 kinds, no 30 — discrepancia
      del Contrato reportada, ver `docs/eventos.md` §0)
- [x] **T0.1** — workspace de Cargo
- [x] **T0.3** — tipos de error (`YuntaError`) + `tracing`
- [x] **T1.1** (recorte) — schema `prompt`/`bash`/`loop` en `yunta-core`
- [x] **T2.1** — storage: SQLite WAL, event log append/read/list (`yunta-storage`)
- [x] **T2.2** — los 31 tipos de payload de evento (`yunta_core::events`)
- [x] **T2.3** — derivación de estado por replay (`yunta_engine::derive`)
- [x] **T3.1** — trait `Adapter`/`AgentSession` (M-0 cut: sin `context`/`skills`/MCP en
      `SessionRequest`) en `yunta-adapters`
- [x] **T3.2** — adapter `mock`: fixtures YAML, sesión exitosa/fallida/colgada,
      ediciones fuera de scope con `edit_hooks`
- [x] **T5.1** — parseo y registro del ledger: las 7 reglas de validación de
      `docs/spec-ledger.md` §3, todos los errores juntos
- [x] **T5.2** — ciclo de la tarea: pre-check en rojo, dispatch, post-check,
      reintentos con sesión nueva, `blocked` (`yunta_engine::run_task`)
- [x] **T5.3** — scope check por `git diff` con glob-matching real (`globset`)
- [x] **T3.3** — enforcement de presupuesto en `dispatch` (`yunta_engine::task_cycle`):
      conteo de tokens vía eventos `Usage` contra `max_tokens`, timeout de wall-clock
      contra `budget.timeout`, corte con `interrupt()` → grace period → `kill()` (A4).
      `DispatchOutcome::BudgetExceeded` distingue el corte del engine de un `Failed`
      reportado por la sesión.
- [x] **T1.4** (recorte) — manifest congelado con hashes canónicos
      (`yunta_core::Manifest` + `yunta_engine::build_manifest`), incluye
      `base_branch`/`base_commit` y contenido de prompts por archivo; sin
      `inputs`/`modo`.
- [x] **Artifacts al cierre de nodo (§4/§4.1)** — existencia, no-vacuidad y hash
      (`artifact_written`); `kind: task-ledger` se parsea, valida (todas las
      violaciones juntas) y registra (`task_registered`)
      (`yunta_engine::close_artifacts`).
- [x] **Templates (recorte de T6.3)** — sintaxis final `{{var}}` con error duro en
      variable indefinida (`render_template`/`template_variables`); M-0 provee
      `{{run.dir}}` (+ `{{worktree}}` en fixtures de `yunta test`).
- [x] **T4.1/T4.5 (recorte)** — `create_run` (run.dir + `manifest.yaml` +
      `run_created`) y `execute_run`: scheduler secuencial por replay
      (`pending→ready→running→done|failed`), despacho por kind
      (`bash`/`prompt`/`loop`), hooks `before`/`after` (`hook_executed`),
      re-rutas §11.2 (`node_rerouted`, retorno automático, `run_paused` al
      agotarse), commit por tarea verificada (§5.5), huérfanos `running`
      reinician al resumir (`run_resumed`, restart_node). `yunta run` y
      `yunta resume` son la misma función sobre el log.
- [x] **T7.1 (parcial)** — `yunta check`, `yunta run <workflow>`,
      `yunta status <run_id>`, `yunta resume <run_id>` reales en el CLI, con
      config en capas (repo > usuario > org, `YUNTA_HOME` como raíz de estado).
      `real_adapters()` construye `claude-code` cuando `runners:` lo nombra en
      la config mergeada; un workflow con `prompt`/`loop` que no resuelve a
      ningún adapter construido se rechaza **antes** de crear el run.
- [x] **T7.9 (recorte)** — `yunta test`: casos en `.yunta/tests/` con
      `workflow`/`fixture`/`expect` (final_state, nodes, tasks), sandbox por caso
      (worktree git, runs root y DB temporales), fixture renderizado con
      `{{run.dir}}`/`{{worktree}}`, el mock suplanta a todo adapter que la config
      nombre. Sin `mode`/`inputs`/`events`/`never` (esperan su schema o T7.9
      completo).
- [x] **T7.3 — adapter `claude-code` real.** Resultó sí ejercitable: el binario
      `claude` está instalado y autenticado en este sandbox (comparte
      infraestructura de sesión con el propio entorno de trabajo — ver nota de
      costo/seguridad abajo). Implementado en `yunta-adapters::claude_code`:
      - `probe()`: `claude --version`.
      - `spawn()`/`resume()`: `claude -p --output-format stream-json --verbose
        [--resume <id>] [--model] [--agent] <permission-args> <prompt>`.
      - Parser (`parse.rs`) puro: `system/init` → `SessionOpened`; bloques
        `text`/`tool_use` de mensajes `assistant` → `Note`/`ToolUse` (digest =
        `file_path`/`path`/`command`/`pattern`/`url` del input, o su hash);
        `thinking` deliberadamente no se expone. El `usage` de la línea
        `result` final (nunca el de líneas `assistant` intermedias, que
        subcuentan tokens de thinking aún no conciliados) es la única fuente
        de `Usage` — un total correcto importa más que poder cortar la sesión
        a mitad de camino, algo que este modo de ejecución por turnos atómicos
        no soporta bien de todas formas.
      - `capabilities()`: `resume_session`/`permission_profiles`/
        `custom_agents`/`usage_reporting` en `true`; `edit_hooks`/`run_tools`
        en `false` — honesto (A6): no hay bloqueo de ediciones en vivo para el
        CLI real todavía, el scope check post-hoc (T5.3) es el límite real.
      - `interrupt()`/`kill()`: `process_group(0)` al spawnear + `kill -SIGNAL
        -- -<pgid>` (A4, exterminio del árbol completo).
      - Mapeo de permisos (`permissions.rs`): las tres opciones obvias se
        probaron en vivo antes de elegir. `bypassPermissions`/
        `--dangerously-skip-permissions` — rechazadas por el CLI corriendo
        como root (contenedor). `dontAsk` — corre sin colgarse pero **deniega
        toda tool call** en vez de permitirla. `acceptEdits` — confirmada en
        vivo: permite Write y Bash sin prompt, corre como root, produce el
        archivo pedido. Es lo que usan los tres `PermissionProfile`;
        `ReadOnly` además restringe `--tools` a un set no-mutante.
      - Tests: 11 en `crates/adapters/tests/claude_code.rs` contra un binario
        `claude` simulado por script (`fixtures/claude_code_stub.sh`) — sin
        red, sin costo, nunca un LLM real en CI (A8). Cubren sesión exitosa,
        fallo con `retryable`, mapeo de `tool_use`, crash sin evento terminal,
        los tres perfiles de permiso, `--resume`, y **exterminio real del
        árbol de procesos** (el stub genera un nieto que `kill()` debe matar
        también). 1 test end-to-end en `crates/cli/tests/run_flow.rs` prueba
        que `yunta run` real dispara el adapter (mismo stub, vía
        `adapters.claude-code.binary` en config).
      - **Smoke test manual con el binario real** (el criterio de aceptación
        de T7.3): workflow de 3 nodos (`bash` → `prompt` con `claude-haiku-4-5`
        pidiendo crear `marker.txt` → `bash` verificando el archivo con
        `grep`) corrido con `yunta run` de verdad. Resultado: **run
        finished**, `marker.txt` creado con el contenido correcto, 18
        tokens de entrada / 262 de salida atribuidos en `status`. El primer
        intento con `--dangerously-skip-permissions` fue el que reveló el
        bloqueo por root; el segundo con `dontAsk` reveló la denegación
        silenciosa; el tercero con `acceptEdits` fue el que funcionó — los
        tres quedan documentados en el módulo como el rationale del mapeo.
      - **Nota de costo/seguridad**: el binario `claude` en este sandbox
        comparte `session_id` e infraestructura con la sesión de trabajo
        actual (no es una instalación aislada con su propia cuota) y cada
        invocación real cuesta dinero de la cuenta de Anthropic — confirmado
        con el usuario antes de gastar (~5 llamadas de prueba, entre
        US$0.004 y US$0.06 cada una, para diagnosticar el mapeo de permisos y
        el bug de `kill`).
      - **Bug real encontrado y corregido en el camino**: `kill -SIGNAL
        -<pgid>` (sin `--`) hace que `procps-ng`'s `kill` no envíe ninguna
        señal — sale con status 0 igual, sin tocar ningún proceso. Sin el
        test que verifica que un nieto del proceso muere de verdad, esto
        habría sido una violación silenciosa de A4 en producción. El fix
        (`kill <signal> -- -<pgid>`) y el test que lo atrapa quedaron en el
        mismo commit.

## Hecho de más, no nombrado explícitamente en el alcance mínimo

- [x] **T0.2** — CI (`fmt` + `clippy -D warnings` + build/test, `.github/workflows/ci.yml`).
      Confirmado corriendo verde en GitHub Actions real (run `32094336162`, rama
      `claude/yunta-m0-bootstrap-60o1u2`) — ver nota de corrección abajo.
- [x] **T1.2** (recorte) — config en capas: `runners`/`adapters`/`storage`/`paths`.
- [x] **T1.3** (recorte) — `yunta check` como función pura en `yunta-engine`.
- [x] **Revisión de calidad autoiniciada** (regla de CLAUDE.md "nada de unwrap()/expect()
      fuera de tests"): 4 violaciones encontradas por grep en código de librería,
      las 4 corregidas — `ledger.rs`/`check.rs` (detección de ciclos: el enum `Color`
      ahora carga la posición en el stack en vez de buscarla con `.position().expect()`),
      `task_cycle.rs` (el branch de timeout ahora carga su propio `Duration` en vez de
      re-derivarlo con `budget.timeout.expect(...)`), `mock/mod.rs` (`events()` llamado
      dos veces ahora degrada a un stream vacío en vez de entrar en panic). Commit
      `7aad540`.
- [x] **T7.8 (recorte)** — `yunta graph <workflow> [--run <id>]`: DAG como Mermaid,
      `depends_on` vs. `on_failure.goto` visualmente diferenciados, estado derivado
      con `--run`. No es scope mínimo de M-0 (es M7 en el Plan completo) — es la
      tarea vehículo del criterio de éxito de M-0, ver arriba. Commits `79c2f39`
      (implementación, escrita por un `claude-code` real) + `f1a0bed` (test).

## M4 — Engine core (en progreso)

- [x] **T4.2 — aislamiento de working tree (§7.3).** `defaults.isolation:
      worktree|none` en la config (`yunta_core::Isolation`, default `worktree`,
      congelado en el manifest vía `ConfigLayer::resolved_isolation()` — sin
      parámetro nuevo en `build_manifest`, para no romper sus ~8 call sites
      existentes). Mecanismo en `yunta_engine::worktree`:
      - `worktree` (default): `prepare_worktree` corre `git worktree add
        <path> -b yunta/<run_id> <base_commit>` — cada run tiene su propia
        rama descartable desde el commit congelado del manifest; dos runs
        concurrentes sobre el mismo repo nunca chocan porque cada uno recibe
        un path bajo `paths.worktrees` (default `~/.yunta/worktrees/<run_id>`)
        distinto. `release_worktree` es un no-op — el worktree queda en disco
        para inspección; `on_finish.cleanup` no existe todavía en el schema
        (fuera de M-0).
      - `none`: exige árbol limpio (`git status --porcelain` vacío) antes de
        arrancar — sin eso, scope-by-diff (T5.3) no puede distinguir el
        trabajo del agente del que ya estaba. Toma un lock file atómico
        (`OpenOptions::create_new`, sin condición de carrera entre dos
        procesos) junto a los metadatos de git (`--git-common-dir`, nunca
        dentro del working tree — si viviera ahí, ensuciaría el propio chequeo
        que lo requiere) para que un segundo run concurrente sobre el mismo
        checkout se rechace explícitamente en vez de pisarse con el primero.
        `release_worktree` borra el lock solo cuando el run termina
        (`RunTerminal::Finished`) — un run pausado retiene el lock porque un
        futuro `resume` es el mismo run lógico, no uno nuevo.
      - `container` no existe como valor del schema (confirmado con el
        usuario en la sesión de M-0 original) — solo `worktree`/`none`.
      - `{{run.worktree}}` sumado a `template_vars` (`crates/engine/src/run/
        node_exec.rs`) junto al ya existente `{{run.dir}}` — con aislamiento
        real, el cwd de ejecución (`ctx.worktree`) y el directorio de estado
        del run (`ctx.run_dir`) dejan de coincidir por primera vez, así que
        un nodo `bash` necesita poder referenciar cuál es cuál.
      - `yunta run`/`yunta resume` (`crates/cli/src/commands/{run,resume}.rs`):
        `run` deriva el path del worktree del `manifest.isolation` recién
        congelado, llama `prepare_worktree` **antes** de crear el run (un
        árbol sucio bajo `none`, o un lock ya tomado, nunca deja un run
        huérfano a medio crear) y pasa ese path — no `cwd` — a `execute_run`.
        `resume` deriva el mismo path por la misma regla desde el manifest ya
        congelado (nunca vuelve a llamar `prepare_worktree`: el worktree o el
        lock ya existen desde el `run` original) y llama `release_worktree`
        en el mismo punto que `run` si el resume termina el run.
      - Tests: 6 en `crates/engine/tests/worktree.rs` (worktree real en
        `base_commit`, dos runs aislados sin colisión, `none` limpio
        bloquea/libera, `none` sucio se rechaza, liberar `worktree` no borra
        nada), 2 en `crates/engine/tests/manifest.rs` (default y explícito se
        congelan), 4 en `crates/core/tests/config.rs` (parseo/merge/rechazo de
        valores desconocidos), 1 en `crates/engine/tests/run.rs`
        (`{{run.worktree}}` resuelve al cwd real del nodo bash), 5 en
        `crates/cli/tests/run_flow.rs` end-to-end (edición del agente nunca
        llega al checkout original bajo `worktree`; dos runs obtienen
        worktrees independientes; `none` rechaza árbol sucio antes de crear
        ningún run; `none` opera directo y libera su lock al terminar;
        **`resume` continúa en el mismo worktree que `run` creó** — no uno
        nuevo, ni el checkout original).
      - **Deuda no cubierta: el lock de `none` no detecta staleness.** Si un
        run bajo `isolation: none` termina de forma abrupta (crash del
        proceso `yunta`, no un `Paused` prolijo) mientras tiene el lock
        tomado, nadie lo libera — `release_worktree` solo corre al final del
        camino feliz de `run`/`resume`. Un humano tiene que borrar a mano
        `<git-common-dir>/yunta-none.lock` antes de que un run nuevo pueda
        arrancar ahí. No hay PID/timestamp en el lock file ni chequeo de
        "¿el proceso que lo tomó sigue vivo?". Gatillo para resolverlo:
        cuando un crash real deje un lock huérfano en la práctica (o antes,
        si se decide que vale la pena el costo de detectar procesos muertos
        de forma portable).

- [x] **T4.1 — scheduler DAG con paralelismo real (`max_parallel_nodes`).**
      Confirmado con el usuario antes de codear (Notion no está espejado en
      `docs/` todavía — leído directo de la fuente): la tarea completa pide
      `pending→ready→running→done|failed|skipped|waiting` y correr el
      workflow de referencia (`build-feature.yaml`) end-to-end. Recorte
      acordado explícitamente:
      - Solo `pending→ready→running→done|failed` — `skipped` depende de
        modos (M9) y `waiting` de gates (M5/T7.2), ninguno existe en el
        schema todavía.
      - Criterio de aceptación cumplido con un fixture propio (nodos
        `prompt`/`bash`/`loop` de fan-out independiente), no con el workflow
        de referencia completo — ese usa `gate`/`check`, fuera del recorte
        de T1.1.
      - **Default de `max_parallel_nodes` cuando `defaults:` no lo
        declara: `1` (secuencial)**, confirmado con el usuario por simetría
        con §5.5 (`concurrency` de loop): el paralelismo multiplica el
        gasto simultáneo de tokens, nadie lo debe descubrir por la factura.
        El comportamiento de M-0 no cambia si nadie toca la config.
      `defaults.max_parallel_nodes` en la config (`yunta_core::DefaultsConfig`),
      congelado en el manifest igual que `isolation`
      (`ConfigLayer::resolved_max_parallel_nodes()`,
      `Manifest.max_parallel_nodes: u32`). Mecanismo en
      `yunta_engine::run::schedule`:
      - `next_action` (una sola decisión) se convirtió en `next_step`, que
        devuelve `ScheduleStep` — `Execute(Vec<(NodeId, u32)>)` en vez de un
        único `Execute { node, attempt }`. El tipo hace irrepresentable
        mezclar una acción de control (`Reroute`/`Pause`/`Finish`/`Broken`)
        con un lote de ejecuciones en la misma decisión — la cáscara
        imperativa (`execute_run`) nunca necesita adivinar cuál de las dos
        cosas recibió.
      - **Orden de prioridad sin cambios** respecto de antes de T4.1: (1)
        nodos `running` huérfanos (todos juntos, sin tope — ya estaban
        comprometidos a correr antes del crash), (2) resolución de UNA
        falla por iteración (re-ruta o pause, igual que siempre), (3) recién
        acá se forma el lote de nodos `ready` frescos, hasta
        `max_parallel_nodes`, en orden de declaración del workflow.
      - **Un lote corre a término junto** — `execute_run` lo despacha con
        `futures::future::join_all` (concurrencia estructurada: nada se
        spawnea suelto, el `.await` del lote entero retiene la propiedad de
        cada ejecución) y no vuelve a preguntarle al scheduler qué sigue
        hasta que **todos** los miembros del lote alcanzan un estado
        terminal de nodo. Es la misma simplificación que `kind: parallel`
        hace explícita con `join: all` (§5.8) — acá implícita para lotes que
        el scheduler arma solo. Interrumpir a un hermano que sigue corriendo
        apenas otro falla es semántica de `join: any`, T4.6, fuera de este
        recorte.
      - `max_parallel_nodes: 0` en la config se clampea a `1` en el
        scheduler en vez de dejar que cualquier nodo listo se muera de
        hambre para siempre — un lote vacío disfrazado de run trabado en
        vez de un error de config visible. Validar `max_parallel_nodes >= 1`
        en `yunta check` sigue sin existir (T1.3 no cubre semántica de
        `defaults:` todavía).
      - **Deuda no cubierta: sin detección de colisión de escritura entre
        ramas del DAG que corren en paralelo por `max_parallel_nodes`.**
        §5.8/D100 solo la exige para `kind: parallel` (nodos declarados a
        mano en un grupo). El riesgo físico es idéntico (mismo worktree
        compartido — T4.2 da un worktree por *run*, no por nodo), pero acá
        no hay warning de `check` si dos nodos con permisos de escritura y
        scope solapado (o sin scope declarado) quedan `ready` al mismo
        tiempo. Gatillo: extender T4.6's warning (D100) a este caso también,
        o decidir explícitamente que el fan-out implícito exige `scope`
        declarado para habilitarse.
      - Tests: 3 en `crates/core/tests/config.rs` (default 1, parseo,
        override repo\>org), 2 en `crates/engine/tests/manifest.rs` (default
        y explícito se congelan), 2 en `crates/engine/tests/run.rs` —
        `independent_nodes_run_concurrently_up_to_max_parallel_nodes` (3
        nodos bash sin dependencia entre sí, `max_parallel_nodes: 2`,
        prueba por sweep-line de intervalos `date +%s%N` que el solapamiento
        máximo real es exactamente 2, nunca 3 — corrida 5 veces seguidas sin
        flakiness) y `max_parallel_nodes_defaults_to_1_and_stays_fully_sequential`
        (mismo mecanismo, config sin `defaults:`, solapamiento máximo 1 —
        documenta que el comportamiento pre-T4.1 no cambió para nadie que no
        configure nada). Las tres tareas de reroute/pausa/orphan-restart
        preexistentes (`crates/engine/tests/run.rs`) siguen en verde sin
        tocarlas, confirmando que el rediseño de `next_action`→`next_step`
        preservó el comportamiento de esos casos.

- [x] **T4.3 — hooks con timeout, `on_failure: fail|warn` y `node_defaults.hooks`.**
      A diferencia de T4.1, el criterio de aceptación completo de la tarea
      **sí** era alcanzable con el schema recortado de M-0 (`hooks:
      {before, after}` ya existía desde T1.1) — no hizo falta pedir un
      recorte nuevo. Dos detalles de forma que el Contrato no fija
      (nombre/unidad exacta del campo de timeout; granularidad del merge
      de `node_defaults.hooks`) se decidieron por analogía con precedente
      ya existente en el propio código, documentados acá en vez de
      preguntados — ver el porqué de cada uno abajo.
      - **Secuencia sin cambios** (ya estaba bien desde antes de T4.3):
        `hooks.before` → sesión/bash/loop → `hooks.after` → verificación
        (scope + artifacts). Un `before` que falla con la política default
        (`fail`) ya abortaba el nodo sin abrir sesión; un `after` fallido
        con `fail` ya fallaba el nodo antes de la verificación. Confirmado
        con dos tests nuevos que resultaron estar **ya en verde** antes de
        tocar código — la deuda real de T4.3 era timeout, `warn` y
        `node_defaults`, no la secuencia en sí.
      - **`HookStep.timeout_seconds: Option<u64>`** (`crates/core/src/workflow.rs`).
        Nombre/unidad no están en el Contrato ("timeout corto configurable"
        sin más detalle) — elegido por analogía: segundos, no minutos como
        `defaults.timeout_minutes` (sesiones enteras), porque un hook es
        "pegamento" de duración corta por diseño (§11.1). Ausente = sin
        timeout, igual que el comportamiento pre-T4.3 (aditivo, cero
        cambio para workflows existentes).
      - **`HookStep.on_failure: HookFailurePolicy` (`fail` default \|
        `warn`)** — distinto del `on_failure.goto` a nivel nodo (re-ruta);
        un hook nunca re-rutea, solo falla o avisa. `warn`: el
        `hook_executed` con el exit code real igual se emite, pero el nodo
        sigue su camino normal.
      - **`Workflow.node_defaults: Option<NodeDefaults>`** con solo `hooks`
        (extiende cuando otro campo lo necesite). Merge **por fase, no
        concatenado**: si el nodo declara su propio `before`/`after` no
        vacío, reemplaza al default entero para esa fase; si lo deja vacío,
        hereda la lista completa del default. Elegido por la misma regla
        "arrays reemplazan" que ya usa el merge de capas de config
        (§2.2/D52) — no está escrito para `node_defaults` específicamente,
        pero mantiene un solo principio de merge en toda la base de código
        en vez de inventar uno nuevo solo para este caso.
      - **Timeout con exterminio de árbol completo (A4)**, no solo del
        proceso `sh`: mismo patrón que la sesión del adapter `claude-code`
        (`process_group(0)` al spawnear + `kill -KILL -- -<pgid>` al
        vencer el timeout) — un hook que backgroundea algo (`comando &`) no
        puede sobrevivir a su propio timeout como huérfano. Verificado a
        mano (`ps` después de correr el test del timeout: cero procesos
        `sleep` remanentes).
      - `exit_code: -2` en `hook_executed` marca un timeout específicamente
        (distinto de `-1`, ya usado para "la plantilla del comando ni
        siquiera renderizó") — ninguno de los dos es un exit code real de
        proceso (0–255), así que no hay ambigüedad con una salida genuina.
      - Tests (todos en `crates/engine/tests/run.rs`, mock adapter/sin red):
        before-hook fallido aborta sin sesión (fixture `sessions: []` —
        si el engine igual abriera sesión, el mock fallaría por una razón
        distinta y el test lo distinguiría), after-hook falla por default,
        after-hook con `warn` deja terminar el nodo, hook que excede su
        timeout falla el nodo **y** el test completo en <3s en vez de
        esperar los 5s del `sleep` (probado 3 veces seguidas sin
        flakiness), `node_defaults.hooks` aplica cuando el nodo no declara
        hooks propios, y un nodo con hooks propios los usa en vez de
        concatenar con el default.

- [x] **T4.4 — re-rutas: criterio de aceptación ya cumplido, sin código
      nuevo.** A diferencia de T4.1/T4.3, revisar el ✓ real de la tarea
      ("el ejemplo lint→fix-lint→lint del Contrato §11.2 pasa con mock")
      contra lo ya construido mostró que **ya estaba hecho** —
      `node_rerouted`, retorno automático y `max_reroutes` se
      implementaron durante el bootstrap de M-0 original (`on_failure.goto`
      era parte del schema recortado desde el principio: "sin re-rutas el
      bootstrap no funciona", ver "Alcance mínimo" arriba), y el test
      `a_failing_bash_node_reroutes_to_its_corrective_node_and_returns`
      (`crates/engine/tests/run.rs`) es exactamente ese ejemplo, ya en
      verde. Las dos piezas de la prosa completa de T4.4 que **no**
      alcanzan a construirse en este recorte siguen bloqueadas, ya
      documentadas en "Pendiente explícito" #8 más abajo: escalación a
      `kind: gate` real (M5/T7.2 — hoy el `run_paused` genérico con razón
      en texto es el único mecanismo de "esperar a un humano" en toda la
      base de código, no una carencia nueva de T4.4) y `node-output` como
      artifact montable por `context:` (M6). Ninguna deuda nueva agregada;
      T4.4 no necesitó tocar código.
- [x] **T4.5 — `on_interrupt: restart_node | fail_if_uncertain` por nodo.**
      `resume_session` (la tercera opción del Contrato, §8.1: retomar la
      conversación del agente vía `session_id`, degradando a
      `restart_node` con warning si el adapter no tiene la capacidad)
      **no** entra al schema en este recorte — nada en el dispatch de
      `node_exec` resume una sesión en recuperación de crash todavía (el
      único resume de sesión que existe hoy es el reintento automático de
      T3.3 dentro de un mismo intento, un caso distinto); aceptar el valor
      en el schema y degradarlo siempre en silencio a `restart_node`
      habría sido emular una capacidad ausente en vez de no ofrecerla —
      justo lo que "degradación explícita, nunca emulación" prohíbe. Mismo
      tratamiento que `isolation: container` en T4.2: no está diseñado
      acá, no entra al enum.
      - `Node.on_interrupt: Option<OnInterrupt>` (nuevo campo de nodo, no
        solo de config — el Contrato es explícito: "cada nodo `prompt`/
        `loop` declara `on_interrupt`"). Este recorte lo aplica a **todos**
        los kinds, `bash` incluido: el riesgo que motiva `fail_if_uncertain`
        (reintentar a ciegas un efecto no-idempotente tras un crash a mitad
        de ejecución) es tan real para un `bash run: "git push && gh pr
        create"` como para una sesión de agente — restringirlo a
        `prompt`/`loop` habría sido más fiel a la letra del Contrato pero
        menos fiel a su propio razonamiento (I13 extendido a "todo nodo
        bajo `restart_node`", texto de §8.1). `defaults.on_interrupt` en
        config es el fallback cuando el nodo no declara el suyo —
        `node.on_interrupt.unwrap_or(config.resolved_on_interrupt())`,
        resuelto en el momento (no se congela un escalar único en el
        manifest como `isolation`/`max_parallel_nodes`, porque acá el
        override es por nodo — el manifest ya congela workflow y config
        completos, alcanza).
      - `schedule::next_step`, etapa 1 (nodos `running` huérfanos):
        antes reiniciaba todos los huérfanos sin condición; ahora, si
        **alguno** de los huérfanos resuelve a `fail_if_uncertain`, el
        resume completo pausa (nombrando esos nodos) en vez de reiniciar
        ninguno — ni siquiera a los huérfanos que sí son `restart_node` en
        el mismo lote. Simplificación deliberada ante una situación de
        crash rara (varios nodos huérfanos con políticas mixtas a la vez):
        "nunca asumas, nunca reintentes con un efecto potencialmente ya
        aplicado" (§8.1) se aplica al lote entero, no nodo por nodo.
      - Verificación de integridad de run.dir/worktree por hashes de
        artifacts (mencionada en §8.1 para el resume a nivel run) sigue
        sin implementar — depende de T2.5 (`event_hash`), ya fuera de
        M-0 (ver "Pendiente explícito" #7).
      - Tests: `a_node_with_on_interrupt_fail_if_uncertain_pauses_instead_of_restarting`
        (mismo crash simulado que el test de `restart_node` ya existente —
        `node_started` sin evento terminal — pero ahora pausa y **nunca**
        emite un segundo `node_started`) más 4 tests de schema/config
        (`crates/core/tests/{workflow,config}.rs`: default ausente,
        parseo de `fail_if_uncertain`, resolución de config). El test
        preexistente de `restart_node` (`a_run_interrupted_mid_node_resumes_by_restarting_the_orphan`)
        sigue en verde sin tocarlo, confirmando que el default no cambió.

- [x] **T4.6 — `kind: parallel` con `join: all|any` y colisión de scope
      (D97/D100, §5.8).** El incremento más grande de M4: primer node kind
      nuevo desde el schema recortado original de T1.1, y la primera
      infraestructura de cancelación real del engine (nada usaba
      `CancellationToken` antes de esto). Confirmado con el usuario de
      antemano construir la tarea completa, `join: any` incluido, en vez
      de recortar la interrupción en vivo para después.
      - `NodeKind::Parallel { join, nodes: Vec<Node> }`
      (`crates/core/src/workflow.rs`) — los hijos son `Node`s comunes:
      mismos hooks/scope/artifacts/runner que cualquier nodo de primer
      nivel, porque T4.6 los despacha a través del mismo `execute_node`
      recursivo, no de un camino separado. Consecuencia deliberada: los
      prompt de un hijo declarado como `{file: ...}` también se congela al
      crear el manifest (`freeze_prompts` en `crates/engine/src/manifest.rs`
      pasó a ser recursivo).
      - **Los hijos no participan del DAG de nivel superior**: no son
        visibles para el `depends_on`/`on_failure.goto` de otros nodos, y
        el propio `on_failure` de un hijo (si lo declara) queda inerte —
        la re-ruta es una decisión del scheduler de nivel superior
        (`schedule.rs`), que nunca ve nada dentro de un grupo. No
        confundirlo con un vacío silencioso: es la misma arquitectura por
        la que el `max_parallel_nodes` de T4.1 tampoco aplica dentro de un
        grupo — `parallel` siempre corre **todos** sus hijos a la vez,
        sin tope, porque es un grupo nombrado a mano, no fan-out
        implícito.
      - **`join: all`** (default): el grupo espera a que todos los hijos
        terminen (`futures::future::join_all`); si alguno falla, el grupo
        falla nombrando ese hijo.
      - **`join: any`**: carrera real con `futures::stream::FuturesUnordered`.
        Al primer hijo que **termina con éxito**, el grupo cancela a los
        demás — nunca a un hijo que ya falló por su cuenta, que
        simplemente queda descartado de la carrera. Interrupción real:
        cada hijo corre bajo un `CancellationToken` propio del grupo
        (`cancel.child_token()`, así un grupo anidado dentro de otro
        cancela en cascada); un `bash` cancelado recibe `SIGKILL` a todo
        su grupo de procesos (mismo patrón que el timeout de hooks de
        T4.3 — `process_group(0)` al spawnear); una sesión de `prompt`
        cancelada usa el `interrupt()` → espera → `kill()` ya existente
        del adapter (mismo mecanismo que el corte por presupuesto de
        T3.3), ahora corriendo por una señal externa además de por
        timeout — `dispatch_session` (`task_cycle.rs`) hace `select!`
        entre el deadline de budget y la cancelación, cualquiera de los
        dos que llegue primero.
      - **`nodes: Vec<Node>` fija la lectura de stdout/stderr de un `bash`
        cancelable con tareas propias** (`crates/engine/src/run/node_exec.rs`,
        `execute_bash`): drenar los pipes recién después de `wait()`
        arriesgaba el deadlock clásico si el comando llenaba el buffer del
        pipe antes de salir — antes esto lo evitaba `.output()` sin que
        nadie lo pidiera explícitamente; al pasar a un `wait()` cancelable
        había que replicar esa garantía a mano con tareas lectoras
        propias, esperadas antes de devolver el resultado (JoinHandle
        retenido, nunca huérfano).
      - **Re-entrancia en resume**: un hijo ya `Finished` (o `Failed` bajo
        `join: all`) nunca se re-despacha; un grupo `join: any` cuyo
        ganador ya haya terminado (crash entre el `node_finished` del hijo
        y el del propio grupo) cierra de inmediato sin volver a correr a
        nadie. Sin esto, cualquier crash a mitad de un `parallel` habría
        duplicado trabajo ya hecho al reanudar.
      - **D100 — colisión de escritura** (`crates/engine/src/check.rs`):
        recorte explícito porque `permissions:` no existe en este schema
        (T5.7) — hoy **todo** nodo es de facto escribible (`bash` sin
        restricción, `prompt`/`loop` siempre con
        `PermissionProfile::Edit`), así que la condición real de D100
        ("dos o más hijos con permisos de escritura") se simplifica a
        "dos o más hijos", honesto dado el schema en vez de una regla más
        angosta de lo que D100 pretende. Dos hijos con scope declarado y
        solapado (heurística de prefijo literal, reusada de la regla 4 del
        ledger vía `ledger::globs_might_overlap`, ahora `pub(crate)`) es
        **error** en `check()`; sin scope declarado en dos o más, es
        **warning** — nuevo `check_warnings()`, función separada de
        `check()` en vez de agregarle severidad a `CheckError`, para que
        nada de lo que ya trata `check()` como "debe estar vacío para
        seguir" tenga que aprender a filtrar. `yunta check` y
        `check_or_refuse` (el gate previo a `run`/`graph`) imprimen los
        warnings por stderr sin bloquear.
      - **Corrección de alcance encontrada al pasar**: la unicidad global
        de ids (`check()`'s `DuplicateNodeId`) solo escaneaba
        `workflow.nodes` de primer nivel — un hijo de `parallel`
        reutilizando un id ya en uso en otro lado habría corrompido la
        derivación de replay (I2: un solo mapa plano `NodeId -> NodeState`)
        sin que `check` lo viera. Ahora `collect_ids` recorre el árbol
        completo, arbitrariamente anidado.
      - Tests: 6 en `crates/core/tests/workflow.rs` (parseo, default de
        `join`), 5 en `crates/engine/tests/check.rs` (id duplicado global,
        overlap = error, disjoint = sin nada, sin scope = warning, grupo
        de un solo hijo nunca advierte), 4 en `crates/engine/tests/run.rs`
        — `join: all` termina cuando terminan todos, `join: all` falla si
        falla un hijo, `join: any` termina con el rápido **y** el lento
        queda con su `touch` final sin ejecutar (prueba de interrupción
        real, no solo de que ganó el rápido), y la re-entrancia de resume
        (corrida 5 veces seguidas sin flakiness; verificado a mano con
        `ps` que no queda ningún `sleep` huérfano tras la cancelación) — y
        1 en `crates/cli/tests/run_flow.rs` end-to-end (el warning D100
        aparece por stderr y el run igual termina).
      - **Deuda documentada, no silenciosa**: un hijo `kind: loop` no es
        cancelable — su propio ciclo de tareas (`run_task`/`dispatch_session`
        dentro de `loop_exec.rs`) recibe un `CancellationToken` que nadie
        dispara nunca, así que sigue corriendo hasta su propio final
        aunque un hermano gane la carrera de `join: any`. Ampliar
        `run_task` (función pública, con su propia suite de tests) para
        aceptar cancelación externa es más superficie de la que este
        recorte tocó — sin tarea asignada, gatillo: alguien necesita de
        verdad un `loop` corriendo dentro de un grupo `parallel`.

## M5 — Verificación (en progreso)

T5.1–T5.3 (parseo/registro del ledger, ciclo de tarea, scope check) ya
estaban hechos desde el bootstrap de M-0 — ver "Alcance mínimo" arriba.
Empezando por el resto (T5.4–T5.14) con el mismo criterio de "full scope"
confirmado por el usuario para T4.6: sin recortar de antemano, documentando
cada decisión de diseño no escrita explícitamente en el Contrato a medida
que aparece.

- [x] **T5.9 — memoización de criterios (§5.4).** Se adelantó respecto del
      orden del Plan porque T5.4 (`baseline_compare`/`coverage_gate`) la
      necesita como dependencia real, no solo la menciona — "ambos entran
      en la memoización de §5.4".
      - `Memo` (`crates/engine/src/task_cycle.rs`), nuevo tipo público:
        cache **dentro del run únicamente** (`Mutex<HashMap<clave,
        exit_code>>`), nunca cross-run — un resume arranca con cache fría
        en vez de intentar sobrevivir el cierre del proceso. Es una
        elección de recorte deliberada: sobrevivir al resume exigiría
        persistir `tree_hash` en `criteria_checked` (cambio de schema de
        evento, con su propio versionado) para poder reconstruir la cache
        por replay; en cambio, cache fría tras un resume solo significa
        volver a verificar una vez más de lo estrictamente necesario —
        seguro (sobre-verificar), nunca el error peligroso
        (sub-verificar con un resultado viejo).
      - **Clave** = `hash(cmd + tree_hash + config_hash)`. El Contrato
        pide además "env declarado" — cae del recorte porque `Criterion`
        no tiene campo `env:` todavía (nada que declarar); documentado en
        el propio doc del tipo, no un silencio.
      - **`tree_hash`**: fingerprint propio (no litado tal cual en el
        Contrato, que deja la implementación abierta — "árbol del índice,
        o commit + diff sucio"): `HEAD` + `git diff HEAD` (tracked) + hash
        de contenido de cada archivo sin trackear (`git ls-files --others
        --exclude-standard`, leído a mano). Deliberadamente conservador:
        una lista de nombres sin contenido (lo que da `git status` solo)
        dejaría pasar un archivo sin trackear que cambia de contenido sin
        cambiar de nombre entre dos checks — el mismo principio de "más
        falsos-recompute que falsos-reuse" que ya rige el heurístico de
        scopes solapados.
      - Aplica **a todo criterio**, no solo a `guard` — el Contrato es
        explícito ("no hay opt-out por criterio"); los criterios propios
        de tarea casi nunca pegan en cache porque su comando es único, y
        eso es exactamente el comportamiento esperado, no un caso
        especial.
      - `CriterionRun.reused: bool` (antes no existía en el tipo interno;
        solo el evento `CriterionResult` ya lo tenía, siempre en
        `false` — `loop_exec.rs`'s `to_results()` lo copiaba a ciegas).
        Ahora `reused` viaja desde la ejecución real hasta el evento.
      - `RunCtx` ahora posee un `Memo` propio (`Memo::new(manifest.config_hash)`,
        construido una vez por `execute_run`) — no un parámetro nuevo en
        `execute_run`, ya tenía todo lo necesario.
      - **`run_task`/`pre_check`/`post_check` cambian de firma pública**
        (nuevo parámetro `memo: &Memo`) — afecta a los ~9 call sites de su
        propia suite de tests (`crates/engine/tests/task_cycle.rs`), todos
        actualizados.
      - **Deuda no cubierta**: el short-circuit con "orden aprendido"
        (ordenar criterios por duración histórica del log, §5.4 párrafo
        final) no se construyó — depende de tener duraciones históricas
        accesibles desde el log, que hoy no se registran por criterio.
        Es una optimización de UX/velocidad, no de corrección (el
        Contrato es explícito: "la heurística solo afecta el orden de
        evaluación — nunca el veredicto"), así que se puede sumar después
        sin tocar la semántica ya construida acá.
      - Tests: 3 nuevos en `crates/engine/tests/task_cycle.rs` (primer
        check ejecuta de verdad, segundo check sobre árbol sin cambios
        reusa — verificado contando líneas en un marker **fuera** del
        repo, para no auto-invalidar el propio tree_hash con el efecto
        secundario del criterio; un cambio real en el árbol invalida y
        fuerza re-ejecución). Los 9 tests preexistentes de `task_cycle.rs`
        y los de `loop_exec`/`run.rs` que dependen de `run_task`
        transitivamente siguen en verde sin tocar su lógica.

- [x] **T5.12 — `kind: findings` (§4.1, D79/D80).** También adelantada:
      `findings_gate` (parte de T5.4) necesita que `derive()` sepa
      contar hallazgos, y `Finding`/`FindingSeverity`/`FindingPostedPayload`
      ya existían desde T2.2 sin consumidor.
      - `ArtifactKind::Findings` (`crates/core/src/workflow.rs`) +
        `FindingsFile { findings: Vec<Finding> }` (nuevo, junto a
        `Finding` en `events/payloads.rs`) — mismo shape que `Ledger`
        (`tasks:` como única clave de tope), esta vez `findings:`.
      - `crates/engine/src/findings.rs` (nuevo módulo, espejo de
        `ledger.rs`): valida id único, `title`/`location`/`detail` no
        vacíos — junta todas las violaciones, nunca la primera sola.
      - `close_artifacts` (`artifacts.rs`) gana la rama `Findings`
        simétrica a `TaskLedger`; `VerifiedArtifact.findings:
        Option<Vec<Finding>>`. Al cerrar el nodo, `close_node`
        (`node_exec.rs`) emite un `finding_posted` por entrada — mismo
        patrón que un `task_registered` por tarea de un `task-ledger`.
      - `RunState.findings: Vec<Finding>` en `replay.rs`: acumula **cada**
        posteo, nunca deduplicado ahí — el log crudo conserva toda
        autoría (§4.1: "sin perder autorías"). La deduplicación
        ("entre reviewers por `location` + título normalizado") vive en
        una función pura aparte, `dedup_findings()`, de consulta — quien
        cuente/muestre hallazgos la llama, replay no la hornea adentro.
        El schema de `Finding` no tiene lista de autores para fusionar
        ahí — por eso "sin perder autorías" se satisface dejando el log
        crudo intacto, no intentando fusionar autorías en la vista
        deduplicada.
      - **Fuera de este recorte, explícito**: la vía en caliente
        (`yunta_post_finding`, MCP por-run, M8) — el Contrato es
        explícito en que ambas vías comparten un solo schema, así que
        cuando M8 la construya, reusa `Finding`/`FindingsError`
        directamente, no un tipo paralelo. Tampoco se tocó `status`/el
        recibo para mostrar "3 findings: 1 blocking, 2 minor" — eso es
        UI de T7.1/T7.5, la data ya está en `RunState.findings` lista
        para que la consuman.
      - Tests: 3 en `crates/engine/tests/artifacts.rs` (parseo válido,
        ids duplicados + título vacío reportados juntos, YAML malformado
        como error tipado), 2 en `crates/engine/tests/replay.rs`
        (acumulación en `RunState`, dedup por location+título
        normalizado conservando la primera aparición), 1 end-to-end en
        `crates/engine/tests/run.rs` (nodo `prompt` con mock que produce
        `findings.yaml`, el run termina y `derive()` ve el finding).

- [x] **T5.4 — `kind: check` con sus tres builtins (§7.1, §7.2, D85).**
      "El engine verifica con su propia data, nunca espera a una persona"
      (eso es `gate`, sigue fuera de este recorte) — sin sesión, sin
      tokens, sin runner.
      - `NodeKind::Check { builtin: CheckBuiltin }` +
        `CheckBuiltin::{BaselineCompare, CoverageGate, FindingsGate {
        max_severity }}` (`crates/core/src/workflow.rs`) — lista cerrada a
        propósito: un builtin de `check` es por definición algo que el
        engine ya puede evaluar con datos que tiene; cualquier otra cosa
        es un nodo `bash` (exit code) o un `executor` (T5.6). Sin builtin
        de presupuesto — `limits:` ya pausa el run por su cuenta (§8.3),
        duplicarlo como check sería redundante según el propio texto del
        Contrato.
      - `BaselineConfig { suite }` / `CoverageConfig { cmd, threshold }`
        (`crates/core/src/config.rs`), campos `baseline`/`coverage` en
        `ConfigLayer` con merge de reemplazo wholesale (ninguno de los dos
        tipos tiene opcionalidad interna que fusionar campo a campo).
      - **`baseline_compare`, desviación documentada del texto literal del
        Contrato**: la prosa describe capturar la baseline "una vez al
        abrir el run" (`docs/eventos.md` §5.3); acá la captura es **lazy**,
        en la primera vez que un nodo `baseline_compare` se ejecuta dentro
        del run — capturar de forma incondicional en `create_run` hubiera
        exigido volverla async en sus cuatro call sites
        (`crates/engine/src/run/mod.rs`, `crates/engine/tests/run.rs`,
        `crates/cli/src/commands/{test,run}.rs`) para un builtin que la
        mayoría de los workflows nunca usa. Efecto observable: el primer
        `baseline_compare` de un run siempre pasa (no tiene aún nada
        contra qué comparar) y emite `baseline_captured`; cada uno
        posterior re-corre `baseline.suite` y falla solo si la baseline
        capturada salió en exit 0 y la corrida nueva no. El evento
        `baseline_captured.hash` se completa (hash del stdout) porque el
        schema ya lo exige desde T2.2, pero la comparación real usa el
        exit code, no el hash — comparar por igualdad exacta de hash sería
        demasiado estricto para una suite real (timestamps, orden de
        líneas no determinista, etc.), y el propio Contrato solo pide
        "algo que pasaba dejó de pasar", no salida idéntica byte a byte.
      - **`coverage_gate`, convención documentada, no leída del Contrato**:
        la prosa solo dice "medido y comparado por el engine" sin fijar
        contrato de parseo. `CoverageConfig`'s doc comment fija la
        convención: el stdout de `coverage.cmd` debe contener un
        `NN[.NN]%` en algún lado; se toma el último que aparece. El
        extractor (`parse_last_percentage`, `node_exec.rs`) es un scanner
        a mano sobre `&str` — no se sumó `regex` como dependencia nueva
        para un escaneo acotado de un solo patrón (CLAUDE.md: "¿alcanza
        std o algo ya presente?").
      - **`findings_gate`**: compara contra `RunState.findings` **crudo**,
        no contra la vista deduplicada de T5.12 (`dedup_findings`) —
        un gate que pregunta "¿existe algo así de grave?" no debería
        arriesgarse a subcontar por un heurístico de dedup pensado para
        reportar, no para decidir. `FindingSeverity` no tenía `Ord`
        propio; en vez de derivarlo sobre el tipo público (que leería raro
        — `Blocking < Note` no es intuitivo), un `severity_rank` privado
        en `node_exec.rs` ordena por declaración (`Blocking` el peor,
        `Note` el menor) y "at or above" se resuelve comparando rangos.
      - Ninguno de los tres builtins pasa por la memoización de T5.9
        (`Memo`): esa cache es específica del ciclo de criterios de tarea
        (un mismo criterio puede re-chequearse varias veces dentro de un
        mismo intento); un nodo `check` se evalúa una sola vez por
        ejecución de nodo, así que no hay repetición que cachear —
        forzar el mismo mecanismo ahí sería una abstracción sin segundo
        uso real.
      - **No cancel-aware**: mismo recorte que un `loop` hijo de un grupo
        `parallel` (T4.6) — un `check` hijo de un `join: any` corre hasta
        su propio final aunque un hermano ya haya ganado. Alcance de T4.6
        fue bash/prompt únicamente; documentado, no un silencio.
      - Tests: 6 en `crates/core/tests/workflow.rs` (parseo de los tres
        builtins, `max_severity` de `findings_gate`, builtin desconocido
        rechazado). 6 end-to-end con mock en `crates/engine/tests/run.rs`
        (`baseline_compare` pasa en su primera corrida y detecta una
        regresión real en la segunda; `coverage_gate` pasa/falla contra
        el threshold; `findings_gate` pasa/falla contra `max_severity`).

- [x] **T5.5 — `progress.md` derivado del log tras cada nodo (§8.2).** Uno
      de los cuatro insumos exactos del contexto de un nodo al arrancar
      ("su prompt renderizado, sus fuentes de contexto, `progress.md`, sus
      skills — nada más", §8.2) — lo escribe el engine, nunca un agente
      (I20). El texto de §8.2 ata la regeneración específicamente a cada
      `node_finished` (no a `node_failed`); el Plan lo dice más suelto
      ("tras cada nodo"), pero el Contrato es la fuente #1 en el orden de
      CLAUDE.md, así que su redacción literal es la que se implementó.
      - **Gap real encontrado, resuelto preguntando, no inventando**: §8.2
        pide que cada entrada lleve "la descripción de una línea declarada
        en el workflow" para ese nodo, pero ningún campo `description:` a
        nivel nodo existía en el schema (solo `Workflow.description`, a
        nivel workflow completo). Consultado con el usuario antes de
        tocar código; eligió agregar el campo. `Node.description:
        Option<String>` nuevo en `crates/core/src/workflow.rs` — un nodo
        sin descripción cae a su propio `id` en `progress.md`.
      - `render_progress(workflow, events)` (`crates/engine/src/progress.rs`,
        nuevo módulo, núcleo funcional puro sin IO): re-deriva el archivo
        **completo** desde `derive()` cada vez, nunca un apéndice
        incremental — mismo principio "el estado es una función pura del
        log" que ya rige todo lo demás en Yunta (I2). §8.2 no usa
        literalmente la palabra "regenerar"/"sobrescribir", pero es la
        única lectura consistente con cómo se deriva cualquier otro
        artifact acá.
      - Tres secciones mecánicas — `## Finished` (con la descripción, el
        `outcome` y cada `artifact:` que produjo, satisfaciendo el
        criterio de aceptación del Plan: "contiene todos los nodos
        terminados con sus artifacts"), `## Failed` (con su `outcome`),
        `## Next` (todo lo que no llegó a terminal, incluyendo los
        `Running` marcados `(running)`) — sin intentar distinguir "listo
        para correr" de "todavía bloqueado por `depends_on`", que exigiría
        traer el grafo del scheduler a una función que hoy solo necesita
        el log; se puede sumar después sin romper el formato.
      - `RunState` gana `artifacts: HashMap<NodeId, Vec<PathBuf>>`
        (`replay.rs`), acumulado desde `artifact_written` — mismo patrón
        que `findings: Vec<Finding>` de T5.12, un nodo sin artifact
        simplemente no tiene entrada.
      - **Punto de escritura imperativo**: un único `write_progress(ctx)`
        en `node_exec.rs`, llamado justo después del `emit` de
        `NodeFinished` dentro de `close_node` — el único funnel real de
        `node_finished` en todo el engine (bash, prompt, `parallel`
        completo y cada hijo, `check`, y el nodo `loop` vía
        `loop_exec::execute_loop` también terminan ahí), así que un solo
        call site cubre a todos los `kind` sin duplicar el gancho por
        cada uno.
      - `progress.md` se escribe en la raíz de `run.dir`, junto a
        `manifest.yaml` — la ubicación que fija el diagrama de layout de
        §2.
      - Tests: 5 puros en `crates/engine/tests/progress.rs` (sin eventos
        → todo bajo `Next`; nodo terminado con descripción/outcome/
        artifact; nodo fallido; nodo `Running` marcado `(running)`; hijos
        de `parallel` listados en los mismos términos que un nodo de tope)
        + 2 end-to-end en `crates/engine/tests/run.rs` (el archivo existe
        en disco tras un run real y trae la descripción declarada; un
        nodo con artifact lo lista).

- [x] **T5.6 — runtime de executors, `kind: executor` (D47/D87).** El
      punto de extensión para lo que ni `bash` (solo exit code) ni la
      lista cerrada de `check` cubren — código externo con un contrato
      JSON por stdio.
      - **Gap real, no una interpretación mía**: D47 fija la forma de alto
        nivel ("JSON por stdin con `with:`, paths del run, env declarado;
        JSON de resultado por stdout; exit code es el veredicto; timeout
        del engine") pero **nunca baja a nombre de campo** — cero
        ejemplos de un nodo `kind: executor` en ningún fixture de Notion,
        cero shape de JSON con campos nombrados, cero sintaxis de
        `timeout` propia. Investigado a fondo en Notion (Contrato, ADRs,
        RFC-0002, "Config y workflows de referencia", Deuda) antes de
        escribir una sola línea de código — confirmado que el gap es real,
        no una lectura apurada. Consultado con el usuario, que pidió una
        propuesta concreta documentada acá en vez de una nueva pregunta o
        de inventar en silencio; **este bloque ES esa propuesta — pendiente
        de que alguien la suba a Notion como revisión real de D47.**
      - **Schema del nodo** (`NodeKind::Executor`, `crates/core/src/workflow.rs`):
        ```yaml
        id: coverage-gate
        kind: executor
        executor: coverage-gate   # nombre en skills.executors
        with: { threshold: 80 }   # opaco, default {}
        timeout_seconds: 30       # opcional
        ```
        `timeout_seconds` ausente = sin enforcement — mismo convenio que
        `HookStep.timeout_seconds` ya usa (T4.3), reutilizado en vez de
        inventar un default nuevo de la nada; D47 solo dice "timeout del
        engine" sin fijar cuánto ni si es opcional.
      - **Registro** (`SkillsConfig`/`ExecutorRegistration`/`ExecutorKind`,
        `crates/core/src/config.rs`), bajo `skills.executors:` — la única
        sintaxis de registro que sí aparece literal en Notion ("Config y
        workflows de referencia"):
        ```yaml
        skills:
          executors:
            - { name: coverage-gate, kind: binary, path: .yunta/bin/coverage-gate }
        ```
        `kind: binary` es un enum de una sola variante hoy, no un string —
        D47 reserva `wasm` como aditivo futuro explícito, así que un
        `match` exhaustivo sobre el tipo revienta en compilación el día
        que se agregue esa variante, en vez de correrla en silencio como
        binario. `skills.paths`/`skills.always` (descubrimiento de skills,
        inyección en contexto) no entran — M6, sin consumidor acá.
      - **stdin** (`build_stdin`, `crates/engine/src/run/executor_exec.rs`):
        ```json
        { "with": {...}, "run": { "dir": "...", "worktree": "..." }, "env": {} }
        ```
        `run.dir`/`run.worktree` reusan los mismos dos nombres que
        `{{run.dir}}`/`{{run.worktree}}` ya exponen en templates
        (`node_exec::template_vars`) en vez de inventar otros. `env`
        siempre presente pero vacío — el schema no tiene `env:` declarado
        a nivel nodo todavía (nada que declarar), pero la clave queda para
        que un executor nunca tenga que ramificar por su ausencia.
      - **stdout**: `{"summary": "..."}`, opcional. El exit code es el
        veredicto (texto literal de D47) — stdout vacío, no-JSON, o sin
        `summary` nunca falla el nodo por sí solo, cae a un outcome
        genérico (`` executor `<name>` exited 0 ``). `artifacts.produces`
        se reusa tal cual (mismo `close_node` que bash/check) — el
        executor que quiere producir un `task-ledger`/`findings` escribe
        el archivo él mismo, sin plumbing nuevo stdout→artifact.
      - **Proceso**: mismo patrón A4 de todo el engine — `process_group(0)`
        al spawn, `kill -KILL -- -<pgid>` en timeout o cancelación (T4.6's
        `join: any`). Verificado con `ps` tras el test de timeout: cero
        procesos `python3` huérfanos.
      - **Resolución de `path`**: relativo a `ctx.worktree` cuando no es
        absoluto (mismo *cwd* que hooks/bash ya usan), no al directorio
        desde el que corre `yunta`.
      - Tests: 4 de schema en `crates/core/tests/workflow.rs`/`config.rs`
        (parseo del nodo con/sin `with`/`timeout_seconds`, `skills.executors`
        parsea `name`/`kind`/`path`, `kind: wasm` rechazado por no existir
        todavía, merge reemplaza `skills.executors` completo). 4 end-to-end
        en `crates/engine/tests/run.rs` con un executor real en Python sin
        dependencias (criterio de aceptación del Plan): ciclo completo
        feliz (`with.threshold` llega, `run.dir` llega, `summary` se lee
        como outcome), exit no-cero falla el nodo citando stderr, timeout
        excedido falla con diagnóstico, nombre no registrado en
        `skills.executors` falla con diagnóstico.

## Decisiones de recorte explícitas (qué quedó afuera y por qué)

- **T1.1**: nodos `prompt`/`bash`/`loop`, más `parallel` desde T4.6 y `check`
  desde T5.4. Sin `gate`/`executor`/`workflow`; sin `context:`, `skills:`,
  `modes:`, `inputs:`, fan-out de `runners:`, `agent:` a nivel nodo,
  `permissions:`, `scope_expansion:`, `coordination:`. Confirmado con el
  usuario.
- **T1.2**: `runners`/`adapters`/`storage`/`paths`, más `baseline`/`coverage`
  desde T5.4. Sin `mcp_servers`, `skills`, `secrets`, `permissions` (con su
  merge invertido, D51). Confirmado con el usuario.
- **T1.3**: solo unicidad de `id`, referencias+aciclicidad de `depends_on` (excluye
  aristas de `on_failure.goto` por I14), targets de `goto` existentes, `runner:`
  resuelto contra `runners:`. Sin coherencia de modos, scopes disjuntos en
  `parallel`/`inherit`, templates, profundidad/aciclicidad del grafo de workflows,
  techos de `permissions`, warning de push a rama base — todo requiere schema fuera
  de alcance (modos, `parallel`, `context:`/templates, `permissions:`, composición).
- **T2.0**: documenta 31 event kinds, no los "30" que dice la prosa del Contrato
  (la tabla real tiene 31 — ver `docs/eventos.md` §0). Reportado; pendiente de que
  el usuario corrija Notion si coincide con la lectura.
- **CLAUDE.md**: dice "I1–I22" en un lugar e "I1–I30" en otro, dentro de la misma
  página de Notion. El usuario va a corregirlo en Notion; mientras tanto se trabaja
  con I1–I30 (el rango real del Contrato).
- **T2.2 corrección post-implementación**: varios payloads repetían `node_id` (o
  `from_node`/`author_node_id`) ya presente en el envelope del evento. Corregido en
  `yunta_core::events` y en `docs/eventos.md` — ver commit `a488b06`.
- **T2.3**: deriva nodos (lifecycle vía `node_started`/`finished`/`failed`), tareas
  (última `task_status_changed`) y tokens totales. No deriva gates ni presupuestos
  más allá del conteo de tokens — no hay `kind: gate` ni enforcement de `limits.*`
  todavía (T3.3), así que no hay nada que derivar de eso hasta que existan.
- **T3.1**: `SessionRequest` sin `context: ResolvedContext` (M6), `skills: Vec<PathBuf>`
  (fuera del recorte de T1.1) ni `run_tools_endpoint` (MCP, M8). `AgentOutcome` y
  `AgentError` quedan `[inferido]` — la spec los nombra sin detallar sus campos;
  minimalistas a propósito hasta que T7.3 (adapter real) muestre qué información hay
  de verdad disponible para reportar.
- **T3.2**: el mock enforce `edit_hooks` sobre un flag `blocked: bool` que el propio
  fixture declara por efecto, no con glob-matching real — evita traer una dependencia
  de globs solo para el mock; el matching real de scope (T5.3) sí lo va a necesitar.
- **T5.1**: la regla 4 (scopes solapados sin dependencia) usa una heurística
  deliberadamente conservadora — compara solo el prefijo literal de cada glob (todo
  antes del primer `*`/`?`/`[`) en vez de álgebra de globs real. Puede marcar como
  solapados dos globs que en la práctica no chocarían (p. ej. `src/*.rs` vs.
  `src/sub/mod.rs`), pero nunca deja pasar un solapamiento real — para un check de
  seguridad, la falsa alarma es el lado correcto del error.
- **T5.2/T5.3**: `run_task()` corre pre-check una sola vez (no se repite en cada
  reintento — solo dispatch+post-check+scope se repiten, coherente con que el
  árbol ya tiene estado de intentos previos). El scope check usa `git diff --name-only
  HEAD` + `git ls-files --others --exclude-standard` (tracked modificados/borrados +
  nuevos sin trackear); requiere que `cwd` sea un repo git con al menos un commit
  (`HEAD` válido) — sin eso, `ScopeCheckError::GitFailed`, nunca un panic ni un
  falso "todo limpio".
- **T7.1 (check)**: sin flags de formato de salida (`--json`) ni de verbosidad —
  agregar cuando algo los necesite.

## Decisiones de diseño resueltas (con el usuario, 2026-08-18)

Las tres preguntas que estaban abiertas se cerraron con la misma directiva:
**nada custom — se incluyen las piezas normativas que faltaban**:

1. **El `loop` encuentra su ledger por el mecanismo normativo (Contrato §5)**:
   un nodo anterior lo produce como artifact `kind: task-ledger`
   (`artifacts.produces`); al cerrar ese nodo el engine lo parsea, valida
   (`yunta_engine::register`, T5.1) y registra cada tarea (`task_registered`).
   El `loop` consulta el estado derivado — nunca un path propio en el schema.
   **El bootstrap incluye el nodo de planificación real** (`kind: prompt` que
   produce `plan.yaml` como `task-ledger`), por decisión explícita del usuario
   que amplía el recorte original del Plan ("las tareas se convierten a ledger
   a mano") — nada custom ni transitorio; el flujo es el final. Requiere entrar
   al recorte: verificación de artifacts al cierre de nodo (§4) + registro de
   `task-ledger` + el mínimo de templates de T6.3 que el bootstrap necesita
   (`{{run.dir}}`). *(Divergencia con la prosa del Plan en Notion — corregirla
   allá es tuyo; Notion es de solo lectura para mí.)*
2. **El ruteo de fixtures del mock es territorio de `yunta test` (T7.9 / §14)**:
   cada caso en `.yunta/tests/` declara su `fixture:`. Se incluye un recorte
   mínimo de T7.9 en M-0. `yunta run --adapter mock` sin fixture disponible
   degrada con error explícito que apunta a `yunta test` — jamás emulación.
3. **T1.4 manifest**: se construyó la versión normativa recortada (workflow
   resuelto + config mergeada + contenido de prompts por archivo + commit base,
   con hashes canónicos SHA-256). `inputs`/`modo` esperan a que existan en el
   schema. Hecho — ver commit `3e44a45`.

## Pendiente explícito (para retomar sin adivinar)

1. **T7.3 — hecho.** Ver el detalle en "Alcance mínimo de M-0" arriba. `skills`
   queda deliberadamente fuera (M6, `context:`/`skills:` no existen en el
   recorte de T1.1) — no es una falla, es scope.
2. **Grupos de config diferidos de T1.2**: `mcp_servers`, `skills`,
   `baseline`/`coverage`, `secrets`, `permissions` (merge invertido, org manda).
   Implementar recién cuando algo los consuma: `baseline`/`coverage` con T5.4
   (`baseline_compare`), `permissions` con T5.7, `skills` con M6, `mcp_servers` con
   M8.
3. **Node kinds diferidos de T1.1**: `gate`, `check`, `parallel`, `executor`,
   `workflow` (composición). Cada uno llega con su milestone correspondiente (M5
   para `check` builtins y gates, M9 para composición) — no antes.
4. **Reglas de `yunta check` diferidas de T1.3**: coherencia de modos, scopes
   disjuntos en `parallel`/`inherit`, templates, profundidad/aciclicidad del grafo
   de workflows, techos de `permissions`, warning de push directo a la rama base.
5. **Event kinds sin consumidor en replay (T2.3)**: `gate_waiting`/`gate_resolved`,
   `questions_answered`, `finding_posted`, `child_run_*`, ampliación de scope,
   `promotion_signaled` — existen como tipos (T2.2) pero `derive()` los ignora
   porque nada los emite todavía. Sumarlos a `RunState` cuando su schema/ciclo
   llegue (gates: M5/T7.2; composición: M9; findings: T5.12).
6. **T1.5** — validación de inputs al crear el run. Depende de `inputs:` en el
   schema, que T1.1 no implementó (fuera del recorte). Diferir hasta que algo
   necesite inputs reales — probablemente cuando el workflow de bootstrap necesite
   parametrizarse.
7. **`event_hash` (T2.5)**: política ya definida en `docs/eventos.md` §3, sin
   implementar — explícitamente fuera de M-0.
8. **El arco que cerraba M-0 está COMPLETO** (los seis pasos: artifacts al
   cierre, templates mínimos, scheduler T4.1, creación de run, CLI
   run/status/resume, `yunta test`). Ver la lista de alcance arriba. Deudas
   menores que dejó, con su gatillo:
   - **T4.2 (worktrees) — hecho**, ver detalle en la sección "M4" abajo.
   - **T2.4 (paths congelados)**: `resume` busca run.dir bajo el `paths.runs`
     de la config *actual* — cambiarlo entre run y resume no está soportado
     hasta T2.4.
   - **Eventos `agent_session_opened`/`agent_message` sin emitir**: el ciclo
     nodo/tarea ya es auditable (runner_resolved, criteria_checked,
     scope_checked, artifact_written); emitir el detalle por sesión requiere un
     emitter dentro de `dispatch_session`. Gatillo: T7.3 o cuando `status`
     necesite mostrar la sesión viva.
   - **`node-output` como artifact (§11.2)**: el stderr de un `bash` fallido va
     hoy en el diagnóstico de `node_failed` (acotado a 20 líneas), no como
     artifact montable por `context:` — eso es de T4.4/M6.
   - **Subgrafo de corrección**: la re-ruta ejecuta solo el nodo destino; "su
     subgrafo" completo llega con T4.4.
   - **`run_paused` por límites de presupuesto de run (§8.3)**: los budgets por
     sesión (T3.3) están; `limits.max_tokens_per_run`/`max_loop_iterations`
     esperan a que `limits:` entre a la config (fuera del recorte de T1.2).
