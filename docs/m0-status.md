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
- [x] **T1.1** (recorte) — schema `prompt`/`bash`/`loop` en `yunta-core`.
      **Recorte cerrado por DI-13**: el ✓ original ("los YAML de referencia
      parsean round-trip") corre como test — `build-feature.yaml` y el
      config de referencia son fixtures reales en
      `crates/core/tests/fixtures/`, con dos deltas marcados en los
      propios fixtures: el fan-out `runners: []` del nodo `review` es de
      T9.4, y los enteros con `_` de `limits` usan la forma canónica sin
      separador (YAML 1.2, decisión registrada en DI-05).
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
      **Recorte cerrado por DI-13**: todos los grupos de la referencia
      parsean y hacen round-trip, cada uno con consumidor o rechazo
      explícito — `version` (validado == 1 al cargar), `defaults`
      completo (`runner` como fallback de nodo, `timeout_minutes` →
      `Budget.timeout`, `on_failure` solo `pause` — otro valor es error
      de check), `skills.paths`/`always`, `adapter_settings` (passthrough
      opaco), `secrets` (única fuente del env de sesión, I12),
      `telemetry` (parse-and-hold sancionado por la propia referencia) y
      `pricing` con la forma `{cost_per_1k_tokens}` de la doc.
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

## M4 — Engine core (completo: T4.1–T4.6)

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
        `release_worktree` borra el lock cuando el run termina
        (`RunTerminal::Finished`) o cuando el usuario lo cancela (DI-08:
        el proceso engine sale, así que retenerlo solo fabricaría un lock
        huérfano) — un run pausado por otra razón lo retiene porque un
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
      - **Staleness del lock de `none` ✓ (cerrado por DI-08)**: el lock
        ahora lleva dueño (`{ "pid": ... }`); ante contención,
        `prepare_worktree` verifica al dueño con `kill -0` — vivo →
        `Locked` como siempre; muerto → lo roba y lo reporta
        (`WorktreePrepared::StoleStaleLock`, warning en el CLI); lock
        legacy vacío/corrupto → rechazo conservador que nombra el archivo
        a borrar a mano (`LockedByUnknown`).

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
      - **Colisión de escritura en el fan-out implícito ✓ (cerrado por
        DI-12)**: D100 extendido a nodos top-level — con
        `max_parallel_nodes` resuelto > 1, cada par escribible sin camino
        de dependencia (clausura transitiva de `depends_on`; una
        dependencia sobre un hijo de `parallel` cuenta como sobre su
        grupo) con scope declarado solapado es **error**, y 2+
        escribibles sin scope declarado producen **un warning por
        componente conexa** (no por par). Con `max_parallel_nodes: 1` no
        aplica: la ejecución es secuencial y las escrituras sucesivas son
        legítimas. Aproximación estática documentada: "sin orden relativo
        declarado" es la regla, jamás simular el interleaving del
        scheduler.
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
- [x] **T4.5 — `on_interrupt` por nodo.** Originalmente solo
      `restart_node | fail_if_uncertain`; **`resume_session` entró
      después, cerrado por DI-23** (dependía de DI-09 registrando el
      `session_id` en el log): un nodo `prompt` huérfano bajo esa
      política encuentra el último `agent_session_opened` de la ventana
      interrumpida (arranque previo sin veredicto — un intento *fallido*
      terminó con respuesta y jamás se resume) y despacha vía
      `adapter.resume(session_id)`; sin capacidad `resume_session` en el
      adapter, o sin sesión registrada (crash antes de abrir), degrada a
      `restart_node` **con evento** `capability_degraded` — la
      "degradación con warning" que D99 mismo pide, nunca un silencio.
      Declararlo explícito en un nodo sin sesión propia
      (`bash`/`loop`/…) es error de `check`
      (`ResumeSessionOnSessionlessNode`); como default de config aplica
      donde hay sesión y significa restart en el resto. El mock scriptea
      `resume()` sirviendo el próximo fixture bajo el MISMO session id y
      registrando el pedido (`resumes_seen`).
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
      - **Hijo `loop`/`check` cancelable ✓ (cerrado por DI-11)**:
        `run_task`/`dispatch_session` propagan el token del nodo
        (`DispatchOutcome::Cancelled` → `TaskOutcome::Interrupted`), y
        `run_command` de los check builtins corre en process group
        propio con `select!` sobre el token. El destino del nodo
        distingue quién canceló: carrera `join: any` → `node_failed`
        "interrupted…" (el grupo cierra); cancelación del usuario (token
        raíz de DI-08) → sin evento terminal — el nodo queda huérfano y
        el resume lo re-trata por `on_interrupt` (§8.1), que es lo que
        hace la cancelación resumible (test e2e: Ctrl-C → resume →
        finished).

## M5 — Verificación (completo: T5.1–T5.14)

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
      - **Orden aprendido (cerrado por DI-15)**: `CriterionResult` y
        `CriterionRun` llevan `duration_ms: Option<u64>` (aditivo D70;
        `reused` → `None`), el `Memo` acumula duraciones observadas por
        comando (intra-invocación — un resume re-aprende en una pasada),
        y el **pre-check** ejecuta en orden de mediana ascendente, sin
        historial al final en orden declarado (D62: fallan-rápido
        primero). El veredicto se computa sobre el conjunto completo,
        así que el orden jamás lo altera — con test de permutación.
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

- [x] **T5.7 — modelo unificado de permisos (§6.1, D51, D72, D105, I18).**
      UN modelo de techos, no mecanismos sueltos: cada nivel solo estrecha
      al anterior, con la inversión de precedencia deliberada (org manda;
      "sin esta inversión, la gobernanza es teatro"). Gobernanza, no
      sandbox — el límite honesto de §6.1 está citado en el rustdoc del
      tipo, no escondido.
      - **Cuatro gaps reales confirmados contra Notion antes de codear**
        (§6.1/D51 fijan el modelo pero no el dialecto de patrones, ni el
        algoritmo concreto de merge, ni el evento de violación, ni el
        `network:` a nivel nodo que el ✓3 del Plan exige). Mismo
        tratamiento que eligió el usuario para T5.6: propuesta concreta
        documentada acá como addendum pendiente de subirse a Notion como
        revisión de D51.
      - **Propuesta 1 — dialecto de patrones** (`crates/engine/src/permissions.rs`):
        glob de string completo, anclado en ambos extremos; `*` matchea
        cualquier secuencia (espacios, pipes y newlines incluidos — el
        propio ejemplo `"curl * | *"` lo exige); todo lo demás literal;
        case-sensitive; sin clases de caracteres ni `?`. `"sudo *"`
        bloquea `sudo rm` y nunca `echo sudo` — mencionar no es escalar.
        Matcher a mano (~30 líneas, backtracking iterativo), sin
        dependencia nueva.
      - **Propuesta 2 — merge invertido** (`crates/core/src/config.rs`):
        el merge computa el modelo más restrictivo de forma conservadora
        (denies se UNEN entre capas — única lista de toda la config que
        acumula en vez de reemplazar; allows no vacíos se intersecan;
        `packs.executors` conserva el más estricto; `network.default` es
        AND). El intento de aflojar NO se traga en silencio:
        `permission_layer_conflicts(capas ordenadas con nombre)` lo
        reporta como error citando ambas capas y el patrón — comparación
        textual a propósito, mecánica y predecible, sin heurística sobre
        solapamiento de globs. Así el runtime nunca corre lo que el techo
        negó incluso si nadie corrió check, y check cumple el ✓1 literal.
      - **Propuesta 3 — el evento de violación ES `node_failed`**: la
        lista de 31 kinds está cerrada (T2.0) y no tiene evento de
        permisos; §6.1 pide "nodo failed citando la regla, con evento".
        La regla viaja en `node_failed.outcome` — cero schema nuevo,
        cumple el texto literal.
      - **Propuesta 4 — `Node.network: Option<bool>`**: el ✓3 del Plan
        nombra "un nodo con `network: false`" pero ningún schema de Notion
        muestra la clave a nivel nodo (solo `permissions.network.default`
        en config y `declares.network` en packs). Mismo caso que
        `Node.description` en T5.5. Declarativa pura (D105): el test E2E
        documenta que el engine NO bloquea — "test que documenta el
        límite, no un bug", palabras del Plan.
      - **Los dos momentos de enforcement**: `check` estático
        (`CheckError::CommandDenied` — bash `run`, hooks de nodo y de
        `node_defaults`, hijos de `parallel` recursivos; matchea el texto
        literal del YAML) y runtime justo antes de ejecutar (bash tras
        render de template — ✓2 cubierto —, hooks con la violación POR
        ENCIMA de `on_failure: warn` (un hook no puede optar por salirse
        de la gobernanza declarándose warn), criterios de tarea (pre-scan
        en `run_task` → tarea `Blocked` citando la regla → el loop cita
        cada razón de bloqueo en su propio fallo — de paso mejora el
        diagnóstico genérico preexistente de tareas bloqueadas), y
        executors (su "comando" es el path resuelto del binario).
        Criterios se escanean en `run_task` (el único punto de entrada
        del ciclo que usa el engine); los helpers sueltos
        `pre_check`/`post_check` quedan como building blocks puros,
        documentado en el rustdoc.
      - **Nivel nodo**: `permissions: read-only|edit|full` (escalar, la
        grafía literal del Contrato) mapeado 1:1 al `PermissionProfile`
        del adapter en `prompt` y `loop` (antes hardcodeado `Edit`).
        Validación de capability (`read_only` sin `permission_profiles`
        del adapter → error en check) queda para el trabajo de resolución
        de runners con capacidades; hoy el perfil viaja en el request.
      - **Refinamiento D100 de regalo**: `check_warnings` ya no cuenta a
        un hijo `permissions: read-only` como escritor — la condición
        real de D100 ("dos o más hijos CON permisos de escritura") por
        fin es expresable; el comentario que lo dejaba anotado como
        placeholder de T5.7 se retiró.
      - **`yunta check` con capas reales**: sin `--config`, check carga
        las mismas capas org→user→repo que un run (`load_named_layers`,
        compartido con `resolve`) y corre el chequeo de conflictos antes
        del merge. Con `--config` explícito (un solo archivo ya mergeado)
        no hay capas que puedan conflictuar.
      - **Fuera de alcance, explícito**: enforcement de `packs.*` (M11 —
        acá solo se parsea y mergea, incluida la política `prompt` que se
        ejerce en `pack add`, nunca en medio de un run); contradicción
        auditable pack-vs-nodo de `network` (M11, necesita `declares`);
        `pack add` sin TTY bajo `executors: prompt` (indocumentado,
        anotado como pregunta).
      - Tests: 5 de config en `crates/core/tests/config.rs` (parseo del
        shape de referencia, unión de denies, más-estricto para
        executors/network, conflicto re-permitir citando capas, narrowing
        sin conflicto), 4 de schema de nodo en `workflow.rs`, 9 del
        matcher en `crates/engine/tests/permissions.rs`, 5 estáticos en
        `tests/check.rs` (bash/hook/hijo de parallel denegados, template
        no es error estático, read-only fuera del conteo D100), 5 E2E en
        `tests/run.rs` (✓2 template-en-runtime citando regla, hook con
        warn no escapa, criterio denegado bloquea citando regla, ✓3
        network declarativa, executor denegado por path) y 2 del binario
        real en `crates/cli/tests/check.rs` (✓1 re-permitir rechazado
        citando ambas capas; comando denegado por capa org). El ✓3 pasa
        sin cambio de comportamiento — es exactamente lo que el Plan pide
        de ese test: documentar el límite.

- [x] **T5.8 — export de `events.jsonl` al cierre (§8.3, §3.1). El
      recorte de `on_finish.distill` lo cerró DI-24 (ADR D107):
      transformación determinista — copia de artifacts declarados a
      `.yunta/knowledge/distilled/<wf>/<run>/` + `provenance.yaml` puro
      del log, commit en la rama del run bajo `worktree`, sin commitear
      jamás bajo `none`, orden distill → run_finished → export →
      cleanup, y el ciclo §8.3→§9.2 cerrado (la fuente `knowledge` lee
      recursivo e incluye lo destilado).** Investigado a fondo en Notion antes de codear;
      confirmado con el usuario que la brecha de `distill` es real y de
      otra naturaleza que las de T5.6/T5.7 — decide si el mecanismo es
      testeable con mock (A8), no solo un nombre de campo.
      - **Por qué se separó**: el propio criterio ✓ del Plan
        ("property test: derivar estado desde el JSONL produce el mismo
        `RunState` que el replay desde la DB") solo testea el export —
        nada en el Plan, el Contrato, las ADRs, ni RFC-0003 testea o
        siquiera define el comportamiento de `distill`. §8.3 da el
        propósito (D20: destilar `plan.yaml`/artifacts en ADRs y
        `CONTEXT.md` bajo `.yunta/knowledge/`) pero nunca el mecanismo —
        ¿sesión de agente (como un nodo `prompt`)? ¿transformación
        determinista del engine? Ninguno de los documentos lo dice, y sin
        eso no hay forma de saber si `distill` es testeable con mock sin
        inventar diseño nuevo — exactamente el tipo de decisión que
        CLAUDE.md pide no tomar en silencio. Queda como deuda explícita,
        no implementado; `on_finish:` no se agregó al schema de
        `Workflow` todavía porque su única razón de ser hoy sería
        `distill`, sin mecanismo.
      - **Lo que sí está completamente especificado y se implementó**:
        `events.jsonl` es el log completo del run, un objeto JSON por
        línea en orden de `seq`, escrito en la raíz de `run.dir` (junto a
        `manifest.yaml`/`progress.md`) — "el run archivado queda
        autocontenido... con vida independiente de `retention_days`"
        (§8.3). `render_events_jsonl` (`crates/engine/src/events_export.rs`,
        núcleo puro sin IO) serializa cada `Event` tal cual con
        `serde_json` — `EventPayload` ya viene con tag interno `kind`
        (T2.2), así que no hace falta un envelope aparte; T2.0 documenta
        `event_hash`/`prev_event_hash` (I26) como T2.5, todavía sin campo
        en `Event`, así que no hay cadena de hashes que preservar en este
        recorte.
      - **Gap real resuelto con propuesta documentada — condición de
        disparo**: §8.3 solo dice "al cierre" sin enumerar qué terminal
        states cuentan, ni si el export depende de que `on_finish:` esté
        declarado. Propuesta: se exporta en **todo** `RunReport` terminal
        que el scheduler ya reconoce hoy — `Finish` y `Pause` por igual
        — de forma **incondicional**, sin importar si el workflow declara
        `on_finish:` (la propia redacción de §8.3 conjuga "destila...
        **y** exporta" como dos acciones a la par, no una condicionada a
        la otra). El camino `ScheduleStep::Broken` queda deliberadamente
        afuera: hoy ni siquiera llega a un `RunReport` (retorna `Err`
        antes), y exportar un log roto es una pregunta de diseño propia
        que no se resolvió por analogía. Documentado en el doc del propio
        módulo `run/mod.rs`, no solo acá.
      - **Regeneración completa, nunca apéndice**: mismo principio que
        `progress.md` (T5.5) — un run que pausa, resume y después termina
        simplemente reescribe el archivo con el log más completo, nunca
        le agrega líneas al que ya existía.
      - Tests: 3 puros en `crates/engine/tests/events_export.rs` (una
        línea JSON por evento en orden, log vacío → string vacío, y el
        property test literal del ✓ — round-trip byte-a-byte de los
        eventos más `derive()` idéntico entre original y JSONL, con
        varios fixtures a mano en el mismo estilo que
        `replay_is_deterministic_across_several_fixtures` de T2.3, ya que
        el workspace no tiene `proptest`/`quickcheck` como dependencia) +
        2 end-to-end en `crates/engine/tests/run.rs` (el archivo existe
        y su replay coincide con el estado del run tras `Finished`;
        también se escribe tras `Paused`, incluyendo el propio evento de
        cierre en ambos casos).

- [x] **T5.10 — paralelismo de tareas del ledger, `concurrency: N` (§5.5,
      D65).** A diferencia de T5.6-T5.9, esta spec vino **completa** —
      Contrato §5.5 y D65 fijan mecanismo, orden de integración,
      interacción con memoización, presupuesto y default, palabra por
      palabra. Solo dos detalles de mecánica git quedaron sin fijar
      (abajo), resueltos con una llamada de ingeniería defendible en vez
      de una nueva pregunta, dado que no decidían si la feature era
      testeable (a diferencia del gap de T5.8).
      - **Un solo camino de código, sin caso especial** ("si el ledger es
        una cadena de dependencias, el lote es de 1 y el comportamiento
        coincide con el secuencial"): `execute_loop` ya no distingue
        `concurrency: 1` de `concurrency: N` — TODO batch pasa por
        worktree-por-tarea + integración serializada, incluido el camino
        que antes era la ejecución secuencial simple. Los 42 tests
        preexistentes (incluido el bootstrap plan→loop→gate) siguen en
        verde sin tocarse, confirmando que la unificación es
        observacionalmente idéntica al comportamiento anterior.
      - **Formación del lote** (`select_batch`): hasta `concurrency` tareas
        `ready` en **orden de declaración del ledger** — nunca de
        finalización. La disjunción de scope entre tareas independientes
        **no se re-chequea acá**: `ledger::register` (T5.1, ya construido
        en el bootstrap) ya rechaza dos tareas sin `depends_on` entre sí
        que declaren scopes solapados, así que cualquier par que pueda
        estar `ready` a la vez ya es disjunto por construcción — reuso
        directo, no lógica nueva.
      - **Worktree por tarea** (`dispatch_task_in_isolation`): un
        `git worktree add` real por miembro del lote (`prepare_worktree`,
        ya existente desde T4.2), derivado del HEAD de integración
        **capturado una sola vez por lote** — todas las tareas del mismo
        lote parten del mismo snapshot. Ubicación:
        `run.dir/task-worktrees/<task_id>-<intento>`; branch
        `yunta/task/<task_id>/<intento>`. El número de intento se deriva
        del log (cuántos `TaskStatusChanged: running` tiene ya esa tarea),
        nunca de un contador en memoria — sobrevive resume sin estado
        extra.
      - **Integración serializada** (`integrate_task`): rebase de la
        tarea sobre el HEAD de integración *actual* (que puede haber
        avanzado por una integración previa del mismo lote) →
        reejecución de criterios y scope ahí → solo entonces
        fast-forward del worktree compartido. "Verde en el árbol
        individual es necesario, nunca suficiente" se cumple literal: el
        veredicto que cuenta es siempre el de esta segunda pasada.
      - **Corrección semántica real, no un detalle menor**: §5.5 dice
        literal "esa tarea **vuelve a ready**" cuando la integración
        rechaza — **no** `Blocked`. Es una distinción real que casi se me
        pasa: `Blocked` (el mismo que ya existía) es el veredicto de
        `run_task` agotando sus propios reintentos — "esta tarea no
        puede con esto". Un rechazo de integración es un artefacto de
        *timing* de la concurrencia, no evidencia de que la tarea no
        pueda: la tarea vuelve a `Pending` y un lote futuro la reintenta
        sola, automáticamente, sin necesitar una decisión humana. El test
        `a_task_green_in_isolation_but_broken_by_a_sibling_s_integration_returns_to_ready`
        verifica la secuencia completa de `TaskStatusChanged` para
        confirmar el `Pending` intermedio, no solo el estado final.
      - **Auditoría del rechazo, sin evento nuevo**: un fallo de
        criterios/scope post-integración ya tiene su propio
        `CriteriaChecked`/`ScopeChecked` (mismo mecanismo del ciclo
        normal) al que apunta `caused_by`. Un conflicto de **rebase**
        (que nunca llega a correr `post_check`) no tenía a qué apuntar —
        resuelto reusando el vocabulario existente: un `CriterionRun`
        sintético (`cmd: "git rebase <head>"`, `exit_code: 1`) vía el
        mismo evento `CriteriaChecked`, en vez de inventar un kind nuevo
        para lo que en el fondo es "un comando falló". Además,
        `tracing::warn!` en el momento del rechazo (Observabilidad desde
        el día uno).
      - **Guards/suites globales "una vez por lote", gratis por
        memoización**: no se escribió lógica de dedup — el propio §5.5 lo
        anticipa ("la memoización lo resuelve sin lógica extra") y T5.9 ya
        estaba construido antes de este task exactamente para esto. No se
        tocó `Memo` en absoluto.
      - **Dos detalles de mecánica git que el Contrato no fija (llamada de
        ingeniería, no un ADR nuevo)**:
        1. *Qué pasa en un conflicto de rebase*: `git rebase --abort` +
           rechazo (arriba). Alternativa descartada: dejar el worktree en
           conflicto sin abortar — más "inspeccionable" pero deja basura
           git real que un intento futuro con el mismo `task_id` pero
           nuevo número de intento no toca (cada intento usa su propio
           directorio/branch), así que no hay razón para no limpiar.
        2. *Reintentos ilimitados de integración*: si dos tareas
           conflictúan de rebase **siempre** entre sí (caso patológico,
           no mencionado por el Contrato), nada en este recorte pone un
           techo — cada lote futuro las reintentaría indefinidamente. No
           se inventó un cap: §5.5 no pide uno, y el caso general ya se
           autolimita (una vez el HEAD de integración avanza, la mayoría
           de las tareas pasan a estar genuinamente rojas en su propio
           pre-check y `run_task` las bloquea por agotamiento real, como
           muestra el propio test de arriba). Documentado como límite
           conocido, no una promesa incumplida.
      - **`concurrency` no tiene default de config**: solo existe a nivel
        nodo (`Option<u32>`), sin `defaults.concurrency` ni
        `node_defaults.concurrency` — inferido de la ausencia total en el
        schema de referencia más la razón explícita del propio §5.5
        ("nadie debe descubrirlo por la factura"), no de una frase
        literal que lo prohíba. Confirmable después si hace falta.
      - **`yunta-adapters` ganó una capacidad real, no un parche de
        test**: `MockAdapter` servía sesiones estrictamente en orden de
        llamada (`AtomicUsize`) — su propio doc decía "deterministic
        because M-0 execution is sequential", supuesto que T5.10 rompe
        por primera vez (dispatch concurrente real de varias sesiones a
        la vez). Como A8 exige que **todo** camino del engine sea
        ejercitable con mock, esto no era negociable: `SessionScript`
        ganó `match_prompt_contains: Option<String>` (selecciona el
        script por substring del prompt entrante — que ya incluye el
        `task_id` vía el template de `run_task`), y `MockAdapter` pasó de
        un contador atómico a un `Mutex<Vec<bool>>` de consumo. Aditivo y
        retrocompatible: un fixture que nunca declara el campo nuevo se
        comporta exactamente igual que antes — verificado con un test
        dedicado (`unmatched_scripts_still_serve_in_declaration_order`).
      - Tests: 3 nuevos en `crates/adapters/tests/mock.rs` (matching por
        substring fuera de orden de llamada, un script matcheado nunca se
        consume dos veces, fixtures sin el campo nuevo mantienen el
        comportamiento de siempre) + 2 de schema en
        `crates/core/tests/workflow.rs` (`concurrency` ausente/declarado)
        + 4 end-to-end en `crates/engine/tests/run.rs`, uno por cada ✓ del
        Plan: reproducibilidad real (8 tareas independientes,
        `concurrency: 4` vs `concurrency: 1`, mismo estado final y
        **misma secuencia exacta de mensajes de commit** — con dispatch
        genuinamente concurrente vía `tokio::join_all` + el mock nuevo);
        una tarea verde en aislamiento que se cae tras la integración de
        otra vuelve a `Pending` sin tocar a la otra (verificado con la
        secuencia completa de `TaskStatusChanged`, no solo el estado
        final); scope de cada tarea evaluado contra su propio diff nunca
        el de su par (inspecciona los `ScopeChecked.diff` de cada tarea
        buscando el archivo de la otra); matar el engine a mitad de lote
        (log escrito a mano: una tarea `Done` ya integrada, otra
        `Running` huérfana) y reanudar reejecuta solo la huérfana —
        confirmado contando cuántas veces la tarea ya-Done recibe
        `Running` en el log tras el resume (debe ser exactamente 1).

- [x] **T5.11 — ampliación de scope, `scope_expansion:` (§6.2, D73).** El
      Contrato fija el modelo entero — tres modos (`rules`/`ask`/`deny`,
      default `deny`), un único request object idéntico en los tres
      (`paths`, `reason`, `proposed_criterion`), el propio pre-check del
      criterio propuesto corriendo primero en todo modo, un cap
      `max_per_run` cuyo agotamiento escala en vez de denegar en
      silencio, y D80 (toda denegación se convierte en finding) — pero dos
      mecánicas de transporte quedan sin fijar, resueltas acá como
      llamadas de ingeniería documentadas (no decidían si la feature era
      testeable, así que no ameritaban una pregunta nueva, siguiendo el
      mismo criterio de T5.10):
      - **Transporte del request**: no existe superficie MCP en este
        codebase (`skills`/`mcp_servers` está fuera de M0), así que no hay
        "tool call" que un agente pueda invocar. Se reusó el patrón que
        T5.12 ya fija para `kind: findings`: un archivo estructurado que
        el agente escribe y el engine lee — acá, un único path conocido
        dentro del worktree aislado propio de la tarea
        (`SCOPE_EXPANSION_REQUEST_FILE =
        .yunta-scope-expansion-request.yaml`), porque un request es un
        objeto por intento, no una lista.
      - **"Tamaño acotado"** (§6.2, sin número): acotado por *cantidad de
        archivos* del diff bajo los `paths` pedidos, no por líneas
        cambiadas — más simple y robusto mezclando archivos trackeados y
        untracked, igual de fiel a "un arreglo chico, adyacente".
        `MAX_EXPANSION_FILES = 5`.
      - **Bug real encontrado y corregido antes de escribir ningún test de
        integración**: el archivo de request, al vivir sin trackear
        dentro del propio worktree de la tarea, aparecía en
        `git ls-files --others` — exactamente lo que `scope_check` (T5.3)
        usa para armar el diff. Sin tratamiento especial, **cualquier**
        uso del mecanismo habría hecho que la propia tarea violara su
        scope por el archivo de control, sin importar el modo ni el
        veredicto — el feature se habría autoderrotado en su primer uso
        real. Corregido en `load_request`: el archivo se borra apenas se
        lee, tratado como señal de control-plane consumida una vez, nunca
        como parte del diff entregable de la tarea — lo cual de paso le
        da a "un request por intento" su único cumplimiento real (una
        segunda lectura contra el mismo worktree no encuentra nada).
      - **`ask` consulta de verdad — resuelto por DI-01** (originalmente:
        "degrada siempre a pausa" porque T7.2 no existía). Hoy
        `Decision::Escalate` llega a `loop_exec`, que arma el objeto §5.3
        (summary con el reason del agente; evidencia mecánica: paths,
        exit del pre-check del criterio propuesto, modo y estado del cap)
        y lo pone a `ctx.human_interaction`. Grant →
        `scope_expansion_granted{decided_by: Person, paths}` (el payload
        ganó `paths`, aditivo D70, para que el grant sea auditable
        autocontenido y el reintento derive su scope efectivo del log,
        I2); deny → `denied{Person}` + finding (D80, misma conversión que
        la vía por regla); en ambos casos la tarea vuelve a `Pending` y
        reintenta. Sin superficie (`NoInteraction`) → pausa idéntica a la
        de antes, sin `gate_waiting` grabado (convención T7.2: una
        pregunta sin resolver re-pregunta en el resume). 3 tests e2e
        nuevos en `run.rs` (grant ensancha el scope del reintento, deny
        genera finding y la tarea cumple dentro del scope original,
        headless pausa igual que siempre).
      - **`max_per_run` bajo concurrencia real (cerrado por DI-16)**: la
        evaluación cara (pre-check del criterio propuesto, reglas,
        diffs) corre 100% concurrente fuera de toda sección crítica;
        solo la ventana del cap — leer conteo → decidir → commitear el
        grant — es atómica (`GrantLedger`, un `tokio::Mutex<u32>` por
        lote sembrado del log; los grants de lotes previos ya son
        eventos porque la integración es serial y termina antes del
        próximo lote). El cap se cumple **exacto** con el dispatch tan
        concurrente como siempre — test: `concurrency: 4`, 4 requests
        simultáneos bajo `rules` con `max_per_run: 2` → exactamente 2
        granted, 2 escaladas.
      - **Un `Done` que igual queda con una decisión pendiente**: si el
        pre-check/mode de una tarea escala (`ask`, o cap agotado) pero
        la propia tarea termina satisfaciendo sus criterios y scope
        *declarados* sin necesitar la ampliación, `run_task` devuelve
        igual `TaskOutcome::Done` — pero con `needs_human_decision: true`.
        `loop_exec.rs` lo respeta: integra la tarea normalmente (el
        trabajo es real) pero pausa el run una vez que el lote completo
        terminó de integrar, citando la tarea, en vez de dejar pasar en
        silencio una escalada que nadie resolvió. Límite honesto
        documentado en el propio módulo: esa pausa es una notificación
        de una sola vez, no estado durable — como la tarea ya quedó
        `Done` e integrada, un resume posterior no la vuelve a
        re-despachar y por lo tanto no vuelve a pausar por el mismo
        request sin resolver. El request sigue siendo auditable para
        siempre en el log (`scope_expansion_requested` sin
        `granted`/`denied` correspondiente es exactamente la huella de
        "todavía debe una decisión"), pero nada en este recorte lo
        vuelve a superficiar automáticamente pasada esa primera pausa —
        explícito en el doc comment de `scope_expansion.rs`, no un hueco
        silencioso (A6).
      - **Mapeo de campos D80 (finding automático por denegación)**: sin
        precedente en el codebase de un finding generado por el engine
        mismo (el único emisor existente, `node_exec.rs`, reenvía
        findings que el propio agente ya escribió con su `id` en un
        artifact `kind: findings`) — decisiones tomadas: `id` =
        `scope-expansion-<task_id>-<intento>` (única por construcción,
        un request por intento); `severity: Minor` (una denegación es
        flujo de control rutinario, no evidencia de que el run esté
        roto — distinta de cualquier severidad que la propia
        falla de criterios/scope de la tarea cargue por su lado);
        `title`/`location`/`detail` arman el mensaje a partir del propio
        `reason` del agente y el motivo de la denegación, nunca una
        explicación inventada por el engine; `proposed_criterion` se
        reenvía tal cual. No cubierto por ningún ADR — llamada de
        ingeniería documentada acá, revisable si el equipo define un
        esquema de ids de finding más adelante.
      - **Excluido deliberadamente de este alcance (gap #1, no
        testeado)**: §6.2 dice que `scope_expansion.mode` sigue el mismo
        modelo de techo que el resto de `permissions` (§6.1, T5.7 —
        capas más bajas solo pueden angostar, nunca aflojar), pero no da
        forma YAML concreta a nivel config para ese ceiling. Ninguno de
        los 5 ✓ del Plan lo ejercita, así que quedó fuera de este task:
        `scope_expansion:` hoy es puramente de nodo (`NodeKind::Loop`),
        sin interacción con `PermissionsConfig`/`merge_permissions`
        (T5.7). Pendiente para cuando el Plan o un ADR lo pida.
      - Tests: 9 en `crates/engine/tests/scope_expansion.rs` (unitarios
        sobre `evaluate` con un repo git real — precheck que ya pasa
        deniega sin consultar en cualquier modo, `deny` deniega sin
        correr regla alguna, `ask` escala, `rules` concede/deniega por
        `within`/criterio-requerido/tamaño, cap agotado escala incluso
        bajo `rules`, cap no agotado no escala) + 3 de schema en
        `crates/core/tests/workflow.rs` (`scope_expansion` ausente,
        declarado con `ask`+`within`+cap, default `deny` sin `mode:`) + 5
        end-to-end en `crates/engine/tests/run.rs`, uno por cada ✓ del
        Plan: escribir fuera de scope sin request nunca es una ampliación
        implícita, ni con un `within` que técnicamente cubriría el path
        (bloquea la tarea, cero eventos `scope_expansion_*`); un criterio
        propuesto que ya pasa se deniega sin consultar incluso en `ask`
        (el run nunca pausa); toda denegación —acá, el default `deny` sin
        ningún bloque `scope_expansion:` en absoluto— produce un finding
        con el `reason` y el `proposed_criterion` del agente; una
        ampliación concedida (`rules`) deja pasar el mismo diff que,
        bajo `deny`, sigue siendo una violación real (dos corridas, mismo
        diff, solo cambia el modo); el request object grabado en el
        evento `scope_expansion_requested` es idéntico —`paths`,
        `reason`, `proposed_criterion`, hasta el resultado del
        precheck— en los tres modos, aunque el veredicto que sigue
        difiera.

- [x] **T5.13 — re-plan: qué sobrevive a un ledger nuevo (§5.7, D84).**
      §5.7 vino completa — regla de identidad (mismo `id`, mismos
      `criteria`, mismo `scope`), qué pasa con lo que cambió (vuelve a
      `pending`), y la garantía de que el trabajo commiteado nunca se
      revierte, palabra por palabra. Ningún gap de mecanismo que
      documentar como llamada de ingeniería esta vez — la única decisión
      real fue *dónde* engancharlo, no *qué* hace.
      - **Disparador: el mecanismo de re-ruta ya construido en T4.4, sin
        una sola línea nueva.** §5.7 dice que un nodo de planificación
        "puede volver a ejecutarse... por una re-ruta" — y `schedule.rs`
        ya sabe re-despachar un nodo de corrección aunque ya haya
        terminado antes (`ScheduleStep::Reroute` seguido de
        `corrective_finished_since` → el nodo que falló "vuelve a ready y
        re-corre", literal de §11.2). Un loop cuyo `on_failure.goto`
        apunta de vuelta al propio nodo `plan` reproduce exactamente el
        escenario de §5.7 sin ningún cambio a `schedule.rs`: la tarea
        rebota (bloqueada, o cualquier otra causa de fallo del loop) →
        el loop falla → re-ruta a `plan` → `plan` corre una sesión
        fresca y sobreescribe su propio artifact → una vez que `plan`
        termina, el loop "vuelve a ready" y su próxima invocación de
        `execute_loop` relee el ledger, ahora con contenido nuevo.
      - **Dónde vive la comparación de identidad**: `node_exec.rs`'s
        `close_node`, el mismo punto donde `TaskRegistered` ya se emite
        por cada tarea del ledger (T5.1) — nunca en `loop_exec.rs`, que
        ni siquiera sabe si el ledger que acaba de leer es el primero o
        el enésimo. Antes de emitir los `TaskRegistered` de esta pasada,
        se arma un mapa `task_id → (criteria, scope)` de la registración
        **más reciente** de cada id ya presente en el log completo
        (`ctx.load_events()`); para cada tarea del ledger nuevo, si ya
        existía con `criteria`/`scope` distintos, se emite
        `TaskStatusChanged { new_status: Pending, caused_by: <seq del
        TaskRegistered recién emitido> }` justo después de registrarla
        de nuevo. `depends_on` queda deliberadamente fuera de la
        comparación — §5.7 nombra solo `id`+`criteria`+`scope`, ninguna
        otra cosa.
      - **"Conserva `done` sin reejecutar" sale gratis, sin código
        nuevo**: `TaskRegistered`'s propio manejo en `replay.rs` ya usa
        `entry(...).or_insert(Pending)` — una segunda registración con
        el mismo id nunca pisa el estado que ya tiene esa tarea. Una
        tarea idéntica entre ledgers simplemente no dispara la
        comparación de arriba (no hay diferencia que reportar), así que
        su `Done` (o cualquier estado que tuviera) queda intacto por
        construcción — la propiedad que el ✓ del Plan pide ya la
        garantizaba T2.3, T5.13 solo necesitaba no romperla al agregar
        la mitad que faltaba (la invalidación).
      - **Por qué el ledger de tareas se puede re-registrar sin violar
        I3 en la práctica**: el archivo `artifacts/plan.yaml` en disco sí
        se sobreescribe cuando `plan` corre de nuevo (la sesión del
        agente escribe al mismo path) — pero el dato que T5.13 necesita
        para comparar identidad nunca se re-lee del archivo: vive
        permanentemente en cada `TaskRegistered` ya escrito al event log
        append-only (I2), que sí es inmutable. La comparación de este
        task lee el log, nunca el archivo viejo.
      - **El trabajo commiteado no se revierte, tampoco por diseño
        nuevo**: nada en el pipeline de integración (T5.10) toca el
        worktree compartido salvo para hacer fast-forward hacia
        adelante; una tarea invalidada por re-plan simplemente vuelve a
        entrar al ciclo normal de `select_batch`/`dispatch_task_in_isolation`
        con un worktree fresco derivado del HEAD *actual* — que ya
        incluye cualquier commit de una tarea hermana intacta. No hay
        "revertir" en ningún código de este engine, así que no hacía
        falta escribir nada para garantizar que no pase.
      - Test: 1 end-to-end en `crates/engine/tests/run.rs` que ejercita
        los tres ✓ del Plan a la vez sobre un solo escenario (más fiel a
        cómo ocurre un re-plan real que separarlos): `task-a` declarada
        idéntica en ambos ledgers nunca vuelve a `Running`; `task-c`
        cambia de criterio (mismo id) y su secuencia de estados es
        `Running → Blocked → Pending → Running → Done` — la re-ruta real
        a `plan`, no una invalidación simulada; el log conserva **ambas**
        registraciones de `task-c` (T5.1's propio criterio de auditoría,
        nunca se pierde una versión); y el commit `"task task-a: Write
        a"` sigue presente en el worktree después de que `task-c`
        termina, confirmando que integrar el trabajo re-planeado nunca
        tocó el de la tarea que sobrevivió intacta.

- [x] **T5.14 — artifact `kind: questions` (§4.1, D86). Alcance recortado:
      parseo/validación, el orden "sesión cerrada antes de renderizar" y
      la pausa `waiting` sin TTY; la materialización real de respuestas
      (`questions_answered` con un canal `tty|mcp|pr` real) queda sin
      implementar.** Investigado a fondo en Notion (§4.1 completo, más
      §5.3 para el objeto de gate que §4.1 dice que las preguntas
      reusan sin TTY) antes de codear. Mismo criterio de separación que
      T5.8 con `distill`: el propio `Channel` (`tty | mcp | pr`) ya viene
      cerrado desde el bootstrap de T2.2 — no hay un cuarto valor que
      inventar para "vía archivo" — y **ninguna de las tres superficies
      reales tiene una sola línea de código en este repo todavía** (TTY
      es T7.1/T7.2, MCP es M8, PR es T7.7): no es una decisión de diseño
      abierta, es una dependencia de milestones futuros que no existe
      para resolver acá. `replay.rs` ya documentaba exactamente este tipo
      de brecha ("nothing emits a `gate_waiting`/`gate_resolved` pair
      until `kind: gate` exists... extend this the day those events
      actually appear") — mismo tratamiento para `questions_answered`.
      - **Por qué se separó así y no de otra forma**: de los 4 ✓ del
        Plan, 3 son puramente sobre lo que el engine hace ANTES de que
        alguien conteste (cierre de sesión, la pausa en sí, resume sin
        estado) — completamente testeables con mock, cero dependencia de
        una superficie de respuesta real. El cuarto (`questions_answered`
        con el canal usado) solo puede probarse simulando una respuesta
        real por algún canal, y como se explica arriba ningún canal
        existe todavía — inventar uno (ej. un archivo de respuestas al
        estilo `SCOPE_EXPANSION_REQUEST_FILE` de T5.11) sería diseñar la
        superficie de respuesta yo mismo sin que el Plan la pida en esta
        tarea, exactamente el tipo de "mejora de la spec al pasar" que
        CLAUDE.md prohíbe.
      - **Tipos nuevos, mismo patrón que `Ledger`/`Task` (no
        `Finding`/`FindingsFile`)**: `Question`/`QuestionsFile`/
        `AnswerType` viven en `crates/core/src/questions.rs`, un módulo
        propio — no en `events/payloads.rs` junto a `Finding`. La
        diferencia real: `FindingPostedPayload` embebe `Finding` directo
        en el evento, pero `QuestionsAnsweredPayload` (ya fijado por
        T2.2) solo lleva `answers_hash: String` — un hash, nunca las
        preguntas ni las respuestas estructuradas — así que `Question` no
        es un tipo de payload de evento, es puro schema de artifact,
        exactamente el lugar de `Ledger`/`Task`.
      - **Validación** (`crates/engine/src/questions.rs`, mirror de
        `findings.rs`): `id` único, `text` no vacío, y `values` no vacío
        cuando `answer_type: choice` — la única regla que §4.1 escribe
        explícitamente más allá del schema. Ningún error se corta en el
        primero; se reportan todos juntos, mismo principio que
        `findings`/`ledger`.
      - **Dónde se engancha la pausa**: `close_node` (`node_exec.rs`), el
        mismo punto donde `task-ledger`/`findings` ya convierten su
        artifact en eventos — nunca antes, porque ese punto solo se
        alcanza después de que la sesión del nodo ya cerró (el propio
        `execute_node` despacha y espera la sesión completa antes de
        llamar a `close_node`), lo cual le da al primer ✓ del Plan su
        cumplimiento gratis, por construcción, sin código nuevo que lo
        garantice. Si el artifact trae preguntas, el nodo nunca llega a
        `node_finished`: en cambio, `fail_with_tokens` con
        `retryable: false` — la misma función que T5.10/T5.11 ya
        reusan para "el run necesita a un humano, no hay nada más
        automático que hacer" — cita cada id de pregunta sin respuesta en
        el diagnóstico. No se agregó ningún estado nuevo a `NodeState`
        ni a `RunTerminal`: `RunTerminal::Paused` ya documentaba su
        propio propósito como "everything done, or waiting on a human" —
        exactamente lo que el Contrato llama `waiting` — así que
        reusarlo es la lectura literal del comentario existente, no una
        interpretación forzada.
      - **Resume sin estado conversacional, también gratis**: un nodo
        `Failed` sin `on_failure.goto` no vuelve a `ready` por sí solo
        (`schedule.rs`, ya construido desde T4.4) — `execute_run` sobre
        un run ya pausado por preguntas simplemente redeclara la misma
        pausa leyendo el log, sin despachar ninguna sesión nueva. Cero
        código nuevo de resume; el test lo prueba pasándole al segundo
        `execute_run` un fixture mock **sin sesiones** — si el resume
        intentara redespachar algo, fallaría con "fixture exhausted" en
        vez de devolver la misma pausa.
      - Tests: 4 en `crates/engine/tests/artifacts.rs` (parseo válido con
        `choice`/`text`, `choice` sin `values` reportado, ids duplicados
        + texto vacío reportados juntos, YAML malformado como error
        tipado) + 2 end-to-end en `crates/engine/tests/run.rs`, cubriendo
        3 de los 4 ✓ del Plan: la sesión mock cierra normalmente
        (`outcome: completed`) y solo entonces el run pausa citando cada
        id sin responder, sin `node_finished` en el log; matar el
        "engine" (representado por invocar `execute_run` de nuevo) y
        reanudar con un fixture sin sesiones reproduce exactamente la
        misma pausa. El cuarto ✓ (`questions_answered` con el canal)
        queda como deuda explícita, con su propio gatillo: cuando exista
        T7.1/T7.2 (TTY) o T7.7 (PR) o M8 (MCP), ese milestone es quien
        cierra este ítem — no antes.

## M6 — Contexto (completo: T6.1–T6.5)

- [x] **T6.1 — trait `ContextSource` + builtins (§9). Alcance recortado
      con varias llamadas de ingeniería documentadas, no un gap único
      como T5.8/T5.14 — la tarea en sí es grande.** §9 fija el modelo
      (siete builtins más `mcp`, materialización efectiva bajo
      `context/<hash>/`, "una fuente que falla es fallo del nodo", evento
      con hash por resolución) pero deja bastante mecánica sin cerrar;
      cada decisión abajo es defendible por separado y ninguna decide si
      la feature es testeable con mock — todas se resolvieron sin
      pregunta nueva, mismo criterio que T5.10/T5.11/T5.13.
      - **Sin `trait ContextSource` real.** El Contrato nombra un trait
        (`id()`/`resolve()`), pero con un solo conjunto de builtins y
        ningún segundo implementador (los packs de M11 son quienes
        necesitarían inyectar una fuente propia) un objeto trait no es
        una frontera real todavía (CLAUDE.md: "una abstracción sin
        segunda implementación real... es costo sin beneficio"). Cada
        builtin es una función resolver detrás de un único `match`
        (`crates/engine/src/run/context_resolve.rs`) — nada impide una
        versión con dispatch dinámico el día que un pack la necesite de
        verdad.
      - **Resuelto para `kind: prompt` y, desde DI-17, `kind: loop`.**
        `context:` vive en `Node` para cualquier tipo; `execute_prompt`
        lo resuelve para su única sesión, y un `loop` lo resuelve **una
        vez por brief de tarea** (`resolve_for_task`): las fuentes
        volátiles (`command`, `run-events`, `ledger`, `node-output`,
        `mcp`) se re-resuelven por brief y las `stable`/`run-stable` se
        memoizan por invocación (`StableContextMemo` — §9.1/D42 da el
        criterio de clases); cada ensamblado emite su
        `context_assembled` con `task_id` (aditivo D70). Un
        `bash`/`check`/`executor`/`gate` no abre sesión — declararlo ahí
        sigue siendo **error de `check`**
        (`CheckError::ContextOnUnsupportedNode`), nunca un silencio —
        A6.
      - **`artifact:` crea de verdad una dependencia implícita, sin
        tocar `check`/`schedule.rs`.** `build_manifest` expande
        `context: [{ artifact: { node, ... } }]` en el propio
        `depends_on` del nodo, una sola vez, antes de calcular
        `workflow_hash` — así que el scheduler y el chequeo de ciclos
        (ambos ya solo leen `Node.depends_on`) no necesitan saber que
        `context:` existe. `check()` corre la misma expansión sobre su
        propio clon antes de buscar ciclos, para que un ciclo formado
        *solo* por referencias `artifact:` cruzadas se detecte estático,
        no como deadlock en un run real — verificado con un test
        dedicado.
      - **`files:` es ruta literal, no glob real.** El único ejemplo del
        Contrato usa dos paths sin comodines; ningún ✓ de T6.1 ejercita
        expansión de patrones; y no hay ninguna librería de *filesystem
        walk* en el workspace todavía (`globset` solo *matchea* contra
        paths ya conocidos, no los enumera). Agregar una dependencia
        nueva (`glob`/`walkdir`) para una capacidad que nada testea
        hubiera sido sobre-ingeniería — cada entrada de `files:` se
        renderiza por template y se lee como un path directo, relativo
        al worktree o absoluto si el template ya lo resolvió así (el
        propio caso `{{run.dir}}/...` del ejemplo). Soporte real de glob
        queda como deuda nombrada, no una aproximación silenciosa.
      - **`knowledge:` resuelve solo la capa `repo`.** T6.5 es
        literalmente la tarea siguiente para `repo > user > org` — pedir
        cualquier capa que no sea `repo` en T6.1 es un error tipado
        (`UnsupportedKnowledgeLayer`), nunca contenido vacío silencioso;
        un directorio `.yunta/knowledge/` ausente sí resuelve vacío sin
        error, porque no tener conocimiento local todavía es el caso
        normal de un repo nuevo, no una fuente rota.
      - **`node-output:` necesitó una captura nueva, no solo un
        lector.** T4.4 ya había dejado esto documentado como deuda
        explícita ("`node-output` como artifact montable por `context:`
        (M6)") — sin captura, el builtin sería un stub imposible de
        testear. Se agregó en `execute_bash` únicamente (el único tipo
        de nodo con stdout/stderr de proceso real ya leído ahí mismo):
        se persiste a `run_dir/node-output/<node_id>.txt`
        **incondicionalmente**, tanto en éxito como en fallo — el caso
        que le importa a §11.2 es exactamente el de un nodo *fallido*
        cuya salida el nodo correctivo necesita leer. Captura para
        `executor`/`prompt` queda fuera, nombrada.
      - **`ledger:` solo resuelve la vista agregada, no "la tarea
        propia".** §9 describe dos variantes para este builtin: la
        propia tarea (para un `executor` dentro de un loop) y el estado
        agregado (para nodos de auditoría). Con `context:` resuelto solo
        para `kind: prompt` (arriba), la variante task-scoped no tiene
        ningún camino de código real que la use todavía — se implementó
        únicamente la agregada (lista `task_id: status` derivada de
        `replay::derive`, ordenada).
      - **Umbral inline/referencia sin config**: mismo tratamiento que
        `MAX_EXPANSION_FILES` en T5.11 — `INLINE_THRESHOLD_BYTES = 4096`
        como constante del engine, documentado como el número que el
        propio §9 deja sin fijar ("umbral configurable"), no una
        promesa de que hoy sea configurable.
      - **`content_hash` agregado a `ContextSourceRef`, `mcp` fuera de
        alcance.** T2.2 (bootstrap) ya había dejado `ContextSourceRef`
        con `source_id`/`kind` pero sin hash — su propio doc comment
        invitaba a esto exactamente ("`kind` stays a plain string until
        M6..."). Se agregó `content_hash: String`, el campo que hace
        literal "cada resolución emite evento con hash" (§9);
        `segment_hashes` (ya existía, por clase de estabilidad §9.1)
        se deja vacío — poblarlo sin que exista clasificación de
        estabilidad todavía sería inventar datos. `mcp` no está en el
        `match` de builtins en absoluto — T6.2 lo agrega aparte, tal
        como el propio Plan separa las dos tareas.
      - Tests: 3 de schema en `crates/core/tests/workflow.rs` (default
        vacío, los siete builtins parseando desde el propio ejemplo YAML
        del Contrato, `knowledge.layers` explícito) + 3 en
        `crates/engine/tests/check.rs` (`context:` en un nodo `bash` es
        error, en un nodo `prompt` no lo es, un ciclo formado solo por
        `artifact:` cruzados se detecta) + 9 end-to-end en
        `crates/engine/tests/run.rs`, uno por builtin más el caso de
        falla: cada uno resuelve su contenido, lo inyecta en el prompt
        (verificado indirectamente pero de forma real — el fixture mock
        solo matchea la sesión si el contenido resuelto aparece
        literalmente en el prompt recibido, así que una resolución rota
        habría hecho fallar el fixture, no solo el assert) y es
        replayable (el archivo materializado bajo
        `context/<content_hash>/content` existe y su propio hash
        recalculado coincide con el que quedó en el evento, sin volver a
        ejecutar nada); el de `artifact:` prueba además que el
        `grill → plan` ordena solo por la dependencia implícita, sin
        `depends_on` explícito; y un `artifact:` que referencia un nodo
        real que nunca produjo el artifact falla el nodo con un mensaje
        que nombra el artifact, nunca contenido vacío.

- [x] **T6.2 — builtin `mcp` (§9). Investigado en la Config y workflows de
      referencia antes de escribir una sola línea de cliente — cambió el
      diseño por completo.** La lectura ingenua de §9 (`mcp: { server,
      query }`) sugiere cualquier transporte; la referencia canónica es
      explícita y distinta de lo que T5.11/T6.1 venían asumiendo para
      "lanzar un proceso": `mcp_servers: { internal-docs: { url:
      "https://...", auth_env: DOCS_TOKEN } }` — streamable-HTTP con
      bearer token, nunca stdio. Sin ese chequeo se habría construido un
      cliente stdio/`transport-child-process` correcto en sí mismo pero
      incompatible con el propio schema que T1.2 ya fija.
      - **Dependencia real, con costo defendido**: `rmcp` (la misma
        librería que T8.1 ya nombra para el lado servidor) con
        `client` + `transport-streamable-http-client-reqwest` +
        `reqwest` (rustls, no OpenSSL) en `[dependencies]` de
        `yunta-engine` — la única forma real de hablar HTTPS con
        MCP desde Rust hoy. `cargo tree -e normal` confirma que esto
        no arrastra ningún componente de *servidor* (`axum` no aparece
        en el árbol normal en absoluto); `tower`/`tower-http`/`hyper`
        sí aparecen, pero son inherentes a `reqwest` mismo — el costo de
        hablar HTTPS en absoluto, no algo que este task eligió de más.
        Ningún `[[bin]]` nuevo, ningún subproceso: el binario de
        `yunta` real solo gana lo que un cliente HTTPS siempre pesa.
      - **Servidor de juguete, deuda cero en el binario real**: `rmcp`
        con `server` + `transport-streamable-http-server`, más `axum`
        (para enrutar el `tower::Service` de rmcp a un
        `TcpListener` real) viven en `[dev-dependencies]` — mismo
        `Cargo.toml`, mismo nombre de crate `rmcp` con features
        distintas por sección; cargo unifica el feature-set para
        `cargo test` pero **`cargo build --release` del binario `yunta`
        nunca las ve** (verificado con `cargo tree -e normal`, que no
        lista `axum`). El servidor de juguete corre in-process, sobre
        `127.0.0.1:0` (puerto asignado por el SO), en el mismo test
        `#[tokio::test]` que lo consume — nada de subproceso stdio, dado
        que el transporte real ya es HTTP.
      - **`query:` → `tools/call` sobre una tool llamada `query`**: ni
        §9 ni la referencia fijan qué verbo MCP dispara `query:`. Se
        interpretó como la lectura más simple del propio nombre del
        campo — invocar `tools/call` con `name: "query"` y el texto
        renderizado como único argumento (`{"query": "..."}`) — y el
        servidor de juguete implementa exactamente esa tool, así que el
        contrato cliente↔servidor de este recorte es autoconsistente
        aunque no esté escrito en ningún doc todavía. Cualquier otra
        tool responde con un `CallToolResult::error`, nunca contenido
        silenciosamente vacío.
      - **`auth_env`, nunca el secreto en config** (I12/O3, ya el
        patrón de `secrets:`): el campo guarda el *nombre* de la
        variable de entorno; el token se lee de `std::env::var` recién
        al resolver, y su ausencia es un error tipado
        (`MissingAuthEnv`) que nombra la variable, chequeado **antes**
        de intentar conectar — mismo orden que `UnknownMcpServer`
        (server no declarado en `mcp_servers:`), ambos verificados sin
        red real de por medio.
      - **Timeout que T6.1 debía y no tenía**: al tocar este archivo se
        notó que `command:` ("stdout con timeout", texto literal de §9)
        nunca había recibido un timeout real en T6.1 — `tokio::process`
        podía colgar el nodo indefinidamente. Corregido acá mismo
        (`EXTERNAL_CALL_TIMEOUT = 30s`, mismo tratamiento sin-número-en-
        el-Contrato que `INLINE_THRESHOLD_BYTES`), compartido con
        `mcp:`. Vicio de T6.1 que no se dejó arrastrar.
      - Tests: 3 end-to-end en `crates/engine/tests/mcp_context.rs`
        (servidor de juguete real vía streamable-HTTP: la respuesta se
        resuelve, se inyecta en el prompt —el fixture mock solo matchea
        si el contenido de la respuesta del servidor aparece
        literalmente ahí— y es replayable con el mismo criterio que
        T6.1; un server no declarado en `mcp_servers:` falla el nodo sin
        ningún intento de red, verificado con un fixture sin sesiones
        disponibles; un `auth_env` cuya variable no existe falla igual
        de temprano, contra una URL que ni siquiera resuelve, probando
        que el chequeo ocurre antes de cualquier conexión) + 1 de schema
        en `crates/core/tests/workflow.rs` (el builtin `mcp` agregado al
        ejemplo completo de los ocho builtins) + 3 en
        `crates/core/tests/config.rs` (`mcp_servers:` parsea la forma de
        la referencia, `auth_env` ausente es un servidor público válido,
        una capa repo reemplaza una entrada entera de `mcp_servers:` sin
        tocar las demás — mismo merge que `runners:`).

- [x] **T6.3 — templates: `{{runner.role}}`, `{{project.*}}` (§9.3).
      Alcance recortado: `{{inputs.*}}` queda explícitamente afuera.**
      `{{run.*}}` ya existía desde T4.x (`render_or_fail`/`run_hook`,
      construidos antes de que este task existiera formalmente); el
      mecanismo genérico de error en variable indefinida
      (`TemplateError::Undefined`, T4.x también) ya cubre **cualquier**
      namespace por diseño — nunca hizo falta código nuevo para que
      `{{project.foo}}` o `{{inputs.bar}}` fallen limpio si nadie los
      define; T6.3 solo necesitaba decidir qué namespaces poblar de
      verdad.
      - **Por qué `{{inputs.*}}` se dejó afuera, explícito, no
        silencioso**: `T1.5` es quien declara el schema de `inputs:`
        (tipos, `required`/`default` excluyentes, validación por tipo,
        existencia de `path`) y quien lo conecta a algo que provea
        valores reales (`--input k=v` es T7.1, todavía sin CLI). Nada de
        eso existe en este recorte. Improvisar ahora un
        `HashMap<String,String>` de inputs sin tipos ni validación,
        solo para que el namespace "funcione", sería diseñar el schema
        de T1.5 sin pasar por esa tarea — exactamente lo que CLAUDE.md
        pide no hacer ("las specs no se mejoran al pasar"). El test
        `an_undefined_inputs_variable_still_fails_the_node_clearly`
        prueba que hoy referenciar `{{inputs.idea}}` ya falla limpio,
        citando el nombre — el comportamiento correcto mientras T1.5 no
        aterrice, no un hueco.
      - **`{{runner.role}}` sin reordenar nada**: el nombre del rol es
        `node.runner` mismo — un `String` ya conocido estáticamente
        desde el propio workflow, nunca el adapter/model que
        `resolve_node_runner` elige después. No hacía falta mover la
        resolución del runner antes del render ni pasar datos nuevos:
        `template_vars` pasó a tomar `node: &Node` (antes solo `ctx`) y
        lee `node.runner` directo, en los 6 call sites que ya existían
        (`render_or_fail`, `run_hook`, y los tres resolvers de
        `context_resolve.rs` que ya recibían `node` — T6.1/T6.2).
      - **`project:` como grupo de config nuevo**: `ProjectConfig {
        name, base_branch, branch_prefix }`, merge por campo
        (repo > user > org, sin techo especial — mismo trato que
        `paths:`/`defaults:`), exactamente los tres campos de la
        referencia. M-0 cut deliberado: nada en este recorte *usa*
        `base_branch` para nada más que templates — D48 (warning ante
        push directo a la rama base) es T1.3 propiamente, sin
        implementar acá.
      - Tests: 2 de schema en `crates/core/tests/config.rs`
        (`project:` parsea la forma de referencia, una capa repo
        reemplaza solo los campos que declara, el resto sobrevive de
        org) + 3 end-to-end en `crates/engine/tests/run.rs`
        (`{{runner.role}}` resuelve al rol declarado del propio nodo;
        los tres campos de `{{project.*}}` resuelven desde una capa de
        config real; `{{inputs.idea}}` sigue fallando limpio, citando el
        nombre, confirmando el recorte de arriba en lugar de solo
        documentarlo).

- [x] **T6.4 — ensamblado estable-primero (§9.1). Clasificación de
      estabilidad más reordenamiento del ensamblado; sin cambios al
      resolver de cada builtin.** §9.1 pide que el prompt final ordene
      sus fuentes stable → run-stable → volatile — nunca en el orden en
      que `context:` las declara — para que el prefijo compartido entre
      sesiones del mismo run (y entre runs con el mismo repo) sea
      byte-idéntico y cacheable por el proveedor del LLM; y que
      `segment_hashes` permita verificar eso mecánicamente en vez de
      confiar en la memoria.
      - **Clasificación por variante, no por config nueva**: `enum
        StabilityClass { Stable, RunStable, Volatile }` con una función
        pura `stability_class(&ContextSpec) -> StabilityClass` —
        `files:`/`knowledge:` son `Stable` (contenido del repo, no
        cambia dentro de un run); `artifact:` es `RunStable` (un
        artifact ya verificado y congelado por I3/I4, fijo desde que se
        escribió pero nuevo en cada run); `command:`/`run-events:`/
        `ledger:`/`node-output:`/`mcp:` son `Volatile` (leen estado que
        cambia mientras el run avanza, o una fuente externa viva). Sin
        tabla de config: la clase es una propiedad del *tipo* de fuente,
        no algo que un workflow declare.
      - **`mcp:` es `Volatile`, decisión explícita.** §9.1 no lo nombra
        en ninguna de sus tres listas de ejemplo (es de T6.2, posterior
        al texto que describe el ensamblado). Se clasificó `Volatile`
        por el mismo principio que ya rige `command:`: una respuesta de
        un servidor externo vivo nunca se puede asumir estable entre
        sesiones, así que asumir lo contrario sería inventar una
        garantía que nadie ofrece.
      - **`resolve_all` reordena, no reordena la resolución en sí**: cada
        fuente se sigue resolviendo en el orden declarado por
        `context:` (ningún cambio de orden de ejecución ni de qué falla
        primero); lo que cambia es a qué balde (`stable_blocks`/
        `run_stable_blocks`/`volatile_blocks`) va cada bloque ya
        renderizado antes de unirlos. El evento `context_assembled`
        sigue listando `sources` en el orden de resolución original —
        solo el texto final ensamblado (lo que ve el LLM) respeta el
        orden por clase.
      - **`segment_hashes` puebla una entrada por clase no vacía**: hash
        del texto canónico ya unido de esa clase (no de cada fuente por
        separado — eso ya lo cubre `content_hash` por fuente desde
        T6.1). Una clase sin ninguna fuente de ese tipo no aparece en el
        mapa — así un test puede afirmar exactamente qué clases entraron
        en juego, sin inventar un hash de string vacío para una clase
        que el nodo ni siquiera declaró.
      - Tests: 1 end-to-end en `crates/engine/tests/run.rs` — dos runs
        del mismo workflow (`files:`+`artifact:`+`command:`) con salida
        de `command:` deliberadamente distinta entre corridas; el
        `content_hash` de la fuente `files:` y el de la fuente
        `artifact:` coinciden bit a bit entre ambos runs (su propio
        contenido nunca cambió), igual que `segment_hashes["stable"]` y
        `["run-stable"]`; `segment_hashes["volatile"]` sí difiere,
        confirmando que el mecanismo distingue clases y no solo repite
        el mismo hash; y las tres claves (`stable`/`run-stable`/
        `volatile`) están presentes porque el workflow de prueba
        ejercita las tres.

- [x] **T6.5 — knowledge layering, `repo > user > org` (§9.2).** T6.1 solo
      resolvía la capa `repo`; §9.2 fija la precedencia completa: "lo del
      repo pisa a lo general ante conflicto". `org` es un pack versionado
      (RFC-0002) sin resolver hasta M11 — pedirlo debe fallar tipado, no
      resolver vacío.
      - **`KnowledgeLayer` como enum cerrado, no `String`** (`Repo | User
        | Org`) — parse-don't-validate: un valor fuera de vocabulario en
        `layers:` (p. ej. `galaxy`) es ahora un error de *parseo* del
        workflow (serde_yaml, `ContextSpec` sigue siendo untagged desde
        T6.1), nunca algo que `resolve_knowledge` descubre en runtime.
        `org` sigue siendo vocabulario válido del enum — su error vive en
        la resolución (abajo), no en el parseo, porque es una capa real
        del contrato que simplemente no tiene implementación todavía.
      - **`yunta_core::user_state_root()` compartido cli/engine.** La CLI
        ya calculaba `$YUNTA_HOME` o `~/.yunta` para cargar `config.yaml`
        (T1.2, antes duplicado como `project::user_root` en
        `crates/cli/src/project.rs`); T6.5 necesita el mismo root para
        que el motor lea `knowledge/` en vivo al resolver contexto. Se
        movió la función a `yunta-core` y la CLI ahora delega en ella —
        una sola fuente de verdad sobre qué es "la capa user", nunca dos
        cálculos que puedan divergir.
      - **Merge por nombre de archivo, `repo` gana el empate.** Las dos
        capas resueltas (`user`, luego `repo`) se acumulan en un
        `BTreeMap` por nombre de archivo; `repo` se aplica después de
        `user` en un orden de precedencia fijo
        (`KNOWLEDGE_PRECEDENCE`), así que un archivo con el mismo nombre
        en ambas capas resuelve a la versión de `repo` sin importar el
        orden en que `layers:` las nombre. `layers:` vacío/ausente sigue
        significando "todas las capas resolubles" (ahora `user` +
        `repo`, `org` nunca incluida implícitamente).
      - **`org` es un error tipado, citando la capa y RFC-0002/M11.**
        `ContextResolveError::UnsupportedKnowledgeLayer` ahora toma un
        `KnowledgeLayer` en vez de `String` (ya no hace falta convertir a
        texto en el sitio del error — `Display` lo hace en el mensaje).
      - **Sin directorio no es error.** Ni `user` (nadie corrió `yunta`
        antes en esa máquina) ni `repo` (repo fresco sin
        `.yunta/knowledge/`) fallan por ausencia de carpeta — mismo
        principio que ya regía T6.1 para `repo` solo.
      - Tests: 2 de schema en `crates/core/tests/workflow.rs` (`layers:
        [repo, user, org]` parsea a las tres variantes en orden; un
        nombre de capa inválido es error de parseo, no de runtime) + 3
        end-to-end en `crates/engine/tests/run.rs` (`layers: [user]`
        resuelve `~/.yunta/knowledge/` vía `YUNTA_HOME` y es replayable;
        `knowledge: {}` sin `layers:` mezcla `repo`+`user`, con `repo`
        ganando un archivo de mismo nombre y el archivo exclusivo de
        `user` sobreviviendo intacto; pedir `layers: [org]` falla el
        nodo citando la capa en el diagnóstico, en vez de resolver
        vacío).

## M7 — CLI y UX (completo: T7.1–T7.10 — T7.3/T7.8/T7.9 ya hechos por M-0)

- [x] **T1.5 — inputs del workflow (§2.3, D82), resuelto desde M7 porque
      T7.1 es su primer consumidor real.** M1 no tiene sección propia en
      este documento (T1.1–T1.4 se hicieron como parte del recorte de
      M-0); T1.5 quedó explícitamente diferida entonces ("Pendiente
      explícito" #6, más abajo) con un gatillo concreto: "cuando algo
      necesite inputs reales". Ese algo es T7.1's `--input k=v` — la
      dependencia M1→M7 del Plan de implementación se cumple acá, no se
      salta.
      - **`InputSpec` como enum etiquetado por `type`, no un struct con
        todos los campos opcionales.** `#[serde(tag = "type")]` con una
        variante por tipo (`String`/`Number`/`Boolean`/`Enum`/`Path`,
        cada una con solo sus propios campos) — parse-don't-validate:
        `min`/`max` en un input `boolean` es irrepresentable por tipo,
        nunca algo que `check` tenga que rechazar en runtime.
      - **`required`/`default` mutuamente excluyentes, con `required`
        implícito cuando ninguno se declara.** D82 dice que tener
        `default` implica no requerido; la lectura simétrica (la única
        consistente) es que la ausencia de `default` implica requerido
        — `required: true` explícito es la forma redundante de decir lo
        mismo. `required: false` sin `default` no tiene valor al que
        caer, así que es un error de `check` (`InputOptionalWithoutDefault`),
        igual que `required: true` + `default` a la vez
        (`InputRequiredWithDefault`).
      - **Validación en dos momentos, nunca solapados.** `check()`
        valida lo que es estático: la forma del propio `InputSpec`
        (conflicto required/default, `enum` sin `values`, `min > max`,
        `pattern` que no compila como regex) y que todo `{{inputs.x}}`
        inline en el workflow (prompt, `bash`/hook `run:`, patterns de
        `files:`, `command:`, query de `mcp:`) refiera a un input
        declarado — reutiliza `template_variables` (T6.3), nunca un
        segundo parser de templates. `resolve_inputs` (nuevo,
        `yunta-engine`) valida lo que necesita datos: tipo, `pattern`,
        `min_length`, `min`/`max`, pertenencia a `values`, existencia de
        `path` — todo antes del primer token, antes de tocar worktree
        (D82's propio "convierte un error caro en uno inmediato").
        `prompt: {file: ...}` queda fuera del escaneo estático de
        `check` — `check` nunca lee archivos (ver el doc del propio
        módulo) — así que un `{{inputs.x}}` no declarado ahí sigue
        fallando recién en runtime, igual que antes de T1.5.
      - **`user_state_root`-style compartido: `yunta_core::user_state_root`
        no aplica acá, pero el patrón de una sola fuente de verdad sí**
        — `build_manifest` gana un parámetro `provided_inputs:
        &HashMap<String, String>` y es el único punto que llama a
        `resolve_inputs`; el resultado (`BTreeMap<String, String>`) se
        congela en `Manifest.inputs` y `template_vars` (T6.3) lo puebla
        como `inputs.<nombre>` — nunca se re-resuelve por nodo (un
        `default` recalculado por nodo sería estado no determinista,
        exactamente lo que D82 prohíbe).
      - **`regex` como dependencia nueva, solo en `yunta-engine`**: la
        única validación de `pattern:` del workspace — se agrega donde
        se usa, no al workspace entero ni a `yunta-core`.
      - Tests: 3 de schema en `crates/core/tests/workflow.rs` (los cinco
        tipos parsean con sus propios campos; round-trip; `inputs:`
        ausente es un mapa vacío, no un error) + 10 de resolución en
        `crates/engine/tests/inputs.rs` (default sin proveer; valor
        provisto pisa el default; requerido sin valor falla nombrando el
        input; `--input` no declarado falla; número con `min`/`max`,
        entero sin `.0` de más; boolean solo `true`/`false`; enum solo
        `values`; string con `min_length`/`pattern`; path validado
        contra un `base_dir` explícito) + 7 de `check()` en
        `crates/engine/tests/check.rs` (los dos conflictos
        required/default; `enum` vacío; `min > max`; `pattern` inválido;
        referencia declarada vs. no declarada, incluida una dentro de
        `context: files:`) + 2 end-to-end (`crates/engine/tests/run.rs`:
        un default resuelve en un `bash` real; `crates/engine/tests/manifest.rs`:
        un requerido sin valor se rechaza en `build_manifest`, antes de
        cualquier trabajo de worktree, y un valor provisto queda
        congelado en el manifest).

- [x] **T7.1 — CLI y UX (§7.1→§8.5), sobre el recorte parcial que ya
      existía (`run`/`check`/`status`/`resume`).** Cubre todo lo listado
      salvo dos piezas nombradas explícitamente como deuda, no como
      olvido — ver abajo.
      - **`--input k=v` (repetible)**: parsea `nombre=valor` a un
        `HashMap`, delega toda validación de tipo/declaración a
        `resolve_inputs` (T1.5) — el parseo del flag y la validación del
        valor son responsabilidades separadas a propósito, para que no
        puedan opinar distinto sobre el mismo error.
      - **`--adapter <nombre>`**: `mock` se rechaza explícito citando
        `yunta test` — un `run` real no tiene fixture que ejecutar
        (`docs/m0-status.md`'s propia entrada de T6.1 ya fija que el
        ruteo de fixtures es territorio de T7.9). Cualquier otro nombre
        debe ser uno de los adapters reales que `real_adapters` ya
        construiría — hoy solo `claude-code`; T7.4 (`codex`) agrega el
        segundo caso real que le da sentido al flag más allá de
        validación.
      - **`--mode <nombre>`**: rechazado siempre, citando que `modes:`
        (§10) no tiene schema todavía (M9). Silenciarlo hubiera
        parecido que el modo se aplicó; rechazarlo es la única opción
        honesta mientras el schema no exista.
      - **`probe()` al crear un run**: Spec Adapter §2 dice que corre
        "en `yunta doctor` y al crear runs" — antes T7.1 solo lo hacía
        `doctor`. `commands::probe_or_refuse` ahora corre en `run`
        también, antes de resolver inputs o tocar el worktree, así un
        adapter roto (binario ausente, versión incompatible, auth
        inválida) se detecta con el mismo costo que cualquier otra
        refusal temprana.
      - **`yunta list` / `yunta list --runs`**: sin servidor. Sin
        `--runs`, recorre `.yunta/workflows/*.yaml` (la misma raíz que
        `yunta test` ya resuelve) y muestra `description` + cada
        `inputs:` con su tipo y si es requerido/opcional — packs (M11)
        extenderían este catálogo, no lo reemplazan. Con `--runs`, cada
        run local con su línea de progreso — comparte función
        (`progress_summary`, movida a `commands::status` para ser
        reusable) con `yunta status` y con el poller de `--follow`, así
        las tres superficies nunca pueden mostrar cosas distintas para
        el mismo log.
      - **§8.5, contadores con contexto, nunca porcentaje**: `X/Y tasks
        · A/B nodes · N reroutes · <fase>` — `Y`/`B` es el tamaño del
        DAG congelado en el manifest (recorriendo `parallel` anidado),
        nunca `state.nodes.len()` (que solo cuenta nodos que ya
        arrancaron). `N reroutes` cuenta eventos `node_rerouted` del
        log. `<fase>` es `waiting — <razón>` para un run pausado — la
        propia razón de `run_paused` ya lee como el ejemplo del
        Contrato ("waiting on gate approve-plan"), así que se reusa
        textual en vez de inventar un segundo vocabulario para el mismo
        hecho.
      - **`yunta run --follow`**: un task en background que relee el
        log cada 500ms y imprime el resumen cuando cambia. §8.5 dice
        "consumiendo el stream de eventos" — esto hace polling, no
        suscripción — `yunta-storage` no expone ningún mecanismo de
        push (D53: interfaz de ~5 métodos, a propósito) y agregar uno
        solo para esto hubiera sido diseñar una superficie nueva de
        storage sin que T2.1 la pidiera. El contenido mostrado es
        idéntico al de un stream real; solo la latencia (acotada por el
        intervalo de poll) difiere. Abre su propia conexión SQLite de
        solo lectura al mismo archivo — WAL (ya activado por
        `Storage::open`) es exactamente lo que hace segura esa segunda
        conexión concurrente con el `Storage` que `execute_run` usa
        para escribir.
      - **`yunta doctor`**: llama `probe()` sobre cada adapter real que
        `runners:` nombra y reporta todos los resultados (a diferencia
        de `probe_or_refuse`, que corta en el primero que falla).
      - **`yunta gc [--dry-run]`**: primer consumidor real de
        `storage.retention_days` (declarado desde T1.2, sin consumidor
        hasta ahora). `release_worktree` es un no-op para
        `isolation: worktree` a propósito (`worktree.rs`: "on disk...
        for inspection") — `gc` es el mecanismo que después reclama ese
        disco: para cada run terminal (`finished` o `broken`) cuyo
        último evento supera `retention_days`, borra `run.dir` y su
        worktree. **Alcance nombrado, no más chico en silencio que lo
        que dice §8.3**: §8.3 también dice que el log base "se conserva
        según `storage.retention_days`", lo que implicaría que las
        filas de la base de datos tienen su propia política de
        retención — pero `yunta-storage` no expone ningún
        delete-events-older-than-X, y agregar uno como efecto
        secundario de este comando hubiera sido exactamente el tipo de
        deuda que CLAUDE.md pide no meter de contrabando. Ese consumidor
        de la retención a nivel de base de datos sigue abierto — ver
        "Pendiente explícito" más abajo.
      - **`yunta cancel <run_id>` — deuda nombrada, no emulada.** Cada
        sesión/hook/executor corre en su propio process group
        (`process_group(0)`) precisamente para que el interrupt→kill
        *interno* (presupuesto, timeout, `join: any`) pueda extinguirlo
        sin llevarse el binario `yunta` — pero eso también significa
        que nada hoy permite que una invocación *separada* de `yunta
        cancel` encuentre y señalice esos procesos: no hay pidfile, ni
        daemon, ni socket. `yunta run --detach` (M8, D101) es el primer
        consumidor real de ese canal y el gatillo natural para
        construirlo. Hasta entonces, `cancel` solo reporta lo que el
        log ya dice: no-op limpio si el run ya terminó/pausó, y una
        refusal explícita — nunca una cancelación fingida — si hay un
        nodo en curso. El propio módulo (`commands/cancel.rs`) documenta
        esto en detalle, incluida la conclusión técnica de que Ctrl-C
        hoy tampoco es un camino limpio (los subprocesos, al estar en su
        propio process group, no reciben el SIGINT que el terminal
        manda al foreground process group) — un gap real, preexistente
        a T7.1 (documentado ya en el módulo `run/mod.rs`: "Crash,
        restart y Ctrl-C son el mismo caso"), registrado acá en vez de
        parcheado de paso, tal como pide CLAUDE.md ("si encontrás un
        problema fuera de tu alcance, registralo, no lo parchees al
        pasar").
      - **"árbol para composición" (§8.5) diferido, no un olvido**:
        composición (`kind: workflow`, child runs) es M9 — no existe
        nada que renderizar como árbol todavía.
      - Tests: 13 end-to-end nuevos en `crates/cli/tests/run_flow.rs`
        (`--input` con default y con override explícito; input
        requerido faltante rechaza antes de crear el run; `--mode`
        rechazado; `list` muestra workflows con sus inputs; `list
        --runs` muestra el resumen de progreso; `doctor` sin adapters
        configurados; `gc` sin `retention_days` configurado, `gc`
        reclamando un run terminal viejo, `gc --dry-run` sin tocar
        nada; `cancel` sobre un run ya terminado; `status` mostrando el
        resumen normativo; `run --follow` imprimiendo al menos una línea
        en curso antes de la línea final).

- [x] **T7.2 — Gates: trait `HumanInteraction` + render en consola del
      objeto de escalación (§5.3).** `GateWaitingPayload`/`GateOption`/
      `GateResolvedPayload` ya existían como tipos de evento desde T2.2,
      sin emisor ni consumidor (`docs/m0-status.md`'s propia entrada de
      T2.3 lo nombraba explícito) — T7.2 es ese primer emisor/consumidor,
      no un tipo nuevo.
      - **`GateOption` corregido a `{id, label, tradeoff}`.** La versión
        de T2.2 (`{option, tradeoff}`) conflaba identidad y texto — §5.3
        los separa (`id: add-store` vs. `label: "Add an in-memory
        session store"`) porque `chosen_option` necesita nombrar algo
        estable, no el texto que un futuro cambio de copy podría romper.
        Sin uso real hasta ahora (T2.2 lo dejó explícito: "nada los
        emite todavía"), así que corregirlo no rompe nada existente —
        exactamente el momento correcto para arreglarlo, antes de que
        algo dependa de la forma vieja.
      - **`HumanInteraction` en `yunta-engine`, la implementación de
        consola en `yunta-cli` (A1).** Un trait, un método
        (`resolve(&self, &GateWaitingPayload) -> Option<GateResolvedPayload>`),
        dyn-safe vía `async-trait` — mismo patrón ya justificado para
        `Adapter`/`AgentSession`. Los mismos tipos de evento son el
        objeto que se renderiza: no hay una versión "de runtime" y otra
        "de log" que puedan divergir — es lo que hace cierto "el mismo
        objeto se renderiza en toda superficie" sin escribirlo dos
        veces. `NoInteraction` (siempre `None`) es la implementación por
        defecto — `yunta test`, los tests del engine y cualquier run
        headless nunca tienen a quién preguntarle.
      - **`None` no es un error — es "no hay superficie viva ahora".**
        `ConsoleInteraction::resolve` chequea `stdin().is_terminal()`
        antes de imprimir nada; sin TTY, `None` inmediato. El llamador
        (`execute_run`) degrada exactamente al comportamiento pre-T7.2:
        pausa citando la escalación, para que `yunta resume` (o, cuando
        exista, un cliente MCP) la resuelva después. Mismo principio que
        `kind: questions` (§4.1) ya aplicaba — "sin TTY... nunca
        cuelga" — aplicado acá por primera vez a una escalación real en
        vez de solo documentado como ausente.
      - **Único gate real de este recorte: re-rutas agotadas (§11.2).**
        Es la única pausa existente con dos desenlaces genuinamente bien
        definidos — reintentar el mismo `goto` una vez más (autorizado
        por el humano, más allá del `max_reroutes` declarado) o abortar
        — a diferencia de una falla plana sin `on_failure` (sigue siendo
        `ScheduleStep::Pause` liso, sin opciones que inventar). La
        función de `schedule.rs` sigue siendo pura: devuelve los hechos
        (`GateExhaustedReroutes { node, goto, max_reroutes, cause }`), y
        es `run/mod.rs` — la cáscara imperativa — quien arma el
        `GateWaitingPayload` y llama a `human_interaction.resolve`.
      - **"retry" reutiliza el mismo mecanismo de re-ruta automática**:
        emite un `node_rerouted` más (attempt = max_reroutes + 1) hacia
        el mismo `goto` — nada nuevo que el scheduler tenga que aprender
        a interpretar. Si el nodo corrector vuelve a fallar, el mismo
        gate se dispara de nuevo (correcto: la autorización es "una vez
        más", no "levantar el techo para siempre"). "abort" pausa el
        run citando la decisión y el `free_text`, si lo hay.
      - **Consola: dos preguntas, nunca una que mezcle ambas cosas.**
        Primero un id de opción válido (repregunta ante cualquier otro
        valor, nunca asume), después una línea de texto libre opcional
        — separado así porque §5.3 dice que `free_text` "siempre
        existe" además del menú, no en su lugar; mezclarlas hubiera
        hecho ambiguo qué significa una respuesta que no calza con
        ningún id.
      - **`kind: questions` con superficie interactiva — resuelto por
        DI-02** (originalmente deuda: T7.2 solo construyó el trait para
        gates). El trait ganó `ask(&QuestionsFile) ->
        Option<QuestionsReply>` con default `None` (método separado de
        `resolve` a propósito: §4.1 y §5.3 son dos formas normativas
        distintas); `ConsoleInteraction::ask` pregunta por pregunta en
        TTY respetando `answer_type`/`required`; el engine valida la
        respuesta completa (`validate_answers` en `yunta-core`, puro),
        materializa `<name>.answers.yaml` como artifact del run (I20) y
        emite `questions_answered{hash, channel, responder}` — el cuarto
        ✓ de T5.14 que faltaba. Respuesta inválida o sin superficie →
        pausa citando exactamente qué falta, igual que antes. El re-ask
        interactivo en un `resume` llega con DI-03 (necesita el estado
        `waiting` derivable).
      - **MCP (`resolve_gate`, M8) no se construyó** — el diseño (un
        trait, un objeto) es lo que garantiza que, cuando M8 lo agregue,
        no haya lógica de gate duplicada que reconciliar; construirlo
        ahora sería adelantar M8 sin que exista el servidor MCP por-run
        (T8.1) que lo expondría.
      - Tests: 3 end-to-end en `crates/engine/tests/run.rs` (un
        `HumanInteraction` de prueba resuelto a "retry" re-rutea al nodo
        indicado y el run llega a `Finished`, con `gate_waiting` y
        `gate_resolved` en el log; resuelto a "abort" pausa citando la
        decisión y el `free_text`; `NoInteraction` reproduce el pausado
        de antes de T7.2 byte a byte — test de regresión explícito) + 1
        en `crates/cli/tests/run_flow.rs` (`yunta run` con stdin
        explícitamente no-TTY, vía `Stdio::null()`, pausa en vez de
        colgarse).

- [x] **T7.4 — adapter `codex` real (probe + spawn + mapeo de sandbox;
      capacidades calculadas en el constructor).** Construido en
      `yunta-adapters::codex`, mismo layout de tres archivos que
      `claude_code` (`mod.rs`/`parse.rs`/`permissions.rs`) y mismo trato
      de A1 (todo lo específico del CLI vive acá, nada se filtra al
      engine).
      - **Sin binario `codex` ni credenciales de OpenAI en este
        sandbox** (`which codex` no encuentra nada) — a diferencia de
        T7.3, que tuvo `claude` instalado y autenticado para probar en
        vivo, acá no hubo forma de correr el CLI real. La mayoría de los
        dominios de documentación oficial (`developers.openai.com`,
        `cookbook.openai.com`) están bloqueados por el proxy de egress de
        este sandbox — pero el código fuente del propio CLI en
        `github.com/openai/codex` no lo está, y es la fuente más
        autoritativa posible sin correr el binario en vivo: el formato de
        wire completo sale directamente de
        `codex-rs/exec/src/exec_events.rs` (los structs `ThreadEvent`,
        `ThreadItem`, `ThreadItemDetails` con sus variantes, citados campo
        por campo en `parse.rs`) y `codex-rs/exec/src/cli.rs` (flags de
        `codex exec`), no de una lectura indirecta. El gist de 81
        invocaciones empíricas y los issues de `github.com/openai/codex`
        siguen citados donde agregan algo que el código fuente por sí
        solo no resuelve (p. ej. confirmar que `thread.started` nunca
        lleva `model` en la práctica, no solo en el tipo). **El criterio
        de aceptación "✓ smoke test manual documentado" queda
        explícitamente sin cumplir por esta razón — no es que se haya
        omitido, es que no hay manera de correrlo desde acá.** Retomar en
        cuanto haya un entorno con el binario y credenciales disponibles.
      - **`probe()`**: `codex --version`, igual que `claude-code`.
      - **`spawn()`/`resume()`**: `codex exec --json [resume <thread_id>]
        [--model] <sandbox-args> <prompt>`. Confirmado (no `[inferido]`):
        `--json`/`--experimental-json` para el stream JSONL; el
        subcomando `resume [--last|<thread_id>]` para continuar una
        conversación; `--model` para el modelo. `codex exec` en sí es no
        interactivo por diseño (aprobación en modo `never` de fábrica,
        según la propia documentación) — a diferencia de `claude -p`,
        no hizo falta encontrar (ni mucho menos probar en vivo) un flag
        que evite que se cuelgue esperando una aprobación.
      - **Parser (`parse.rs`) puro**, mismo criterio "línea no
        reconocida → sin eventos, nunca error" que `claude_code`:
        `thread.started` → `SessionOpened` (el `thread_id` es el
        `session_id`); `item.completed` mapea cuatro variantes de
        `ThreadItemDetails` a `ToolUse` — `command_execution` (digest =
        el comando), `file_change` (digest = el primer `path` de
        `changes`, un item puede tocar varios), `mcp_tool_call` (digest =
        `"{server}:{tool}"`), `web_search` (digest = la query) — todas
        con fallback al hash del item si el campo esperado falta; `type:
        agent_message` → `Note`. `reasoning` deliberadamente no se
        expone (mismo trato que `thinking` en Claude); `todo_list` y el
        `error` de mid-turn (distinto de `turn.failed`) tampoco, por
        falta de precedente en cualquier sentido — más angosto de lo que
        podría ser, nunca más ancho que lo confirmado. `turn.completed` →
        `Usage` (`input_tokens`/`output_tokens`/`cached_input_tokens`,
        nombre de campo literal, sin el "cache_read_..." de Claude) +
        `Completed`. `turn.failed` → `Failed { retryable: true }`
        (`[inferido]`, mismo default que `claude_code` usa para su
        propio caso no documentado — el `max_retries` de yunta ya acota
        el costo de una mala apuesta).
      - **`SessionOpened.model` no sale del stream — es un gap
        confirmado del CLI, no una decisión de este adapter**:
        `thread.started` no lleva `model` (issue abierto
        `openai/codex#14736`, comparado explícitamente ahí con
        Claude Code y Gemini CLI, que sí lo incluyen). El modelo *pedido*
        (`req.model`, o `"default"` si no se pidió ninguno) es lo único
        que hay para poblar el campo que O1 exige.
      - **Mapeo de sandbox (`permissions.rs`)**: `-s`/`--sandbox`
        confirmado con tres valores — `read-only` (ReadOnly),
        `workspace-write` (Edit), `danger-full-access` (Full) — mapeo
        directo, uno a uno, tal como pide la Spec Adapter ("codex...
        permission_profiles mapea a sus modos de sandbox"). Sin
        confirmación en vivo, a diferencia del mapeo de permisos de
        `claude_code` (probado empíricamente antes de elegir).
      - **`capabilities()`**: `resume_session`/`permission_profiles`/
        `usage_reporting` en `true` (todos confirmados por
        documentación); `custom_agents` en `false` — honesto (A6):
        `codex exec` no tiene documentado ningún selector de agente
        nombrado equivalente a `--agent` de Claude Code, así que no hay
        nada que mapear; `edit_hooks`/`run_tools` en `false`, mismo
        motivo que `claude_code`.
      - **`resume_session` fijo en `true`, no "calculado en el
        constructor a partir de `probe()`" literalmente.** La prosa de
        la Spec Adapter para `codex` sugiere ese cálculo, pero
        `Adapter::new` es síncrono y `capabilities()` no tiene forma de
        usar un resultado async de `probe()` sin volver asíncrono el
        constructor — cambio que ni `claude_code` intenta pese a que el
        mismo párrafo de la spec lo enmarca en general. `codex exec
        resume` es un subcomando documentado y estable a la versión del
        CLI contra la que se escribió esto, así que declarar la
        capacidad fija es preciso hoy; version-gating real queda
        pendiente de una razón concreta para pagar el costo de un
        constructor async atravesando `real_adapters`.
      - **`--adapter <nombre>` (T7.1) y `real_adapters` ahora reconocen
        `codex`** — el flag deja de ser solo validación sin efecto
        alternativo real: con dos adapters reales construidos, nombrar
        uno explícito empieza a tener un segundo caso genuino que
        distinguir, no solo el primero.
      - Tests: 15 en `crates/adapters/tests/codex.rs` contra un binario
        `codex` simulado por script (`fixtures/codex_stub.sh`, mismo
        diseño que el stub de Claude) — sesión exitosa con `Usage` y
        `Completed`, turno fallido con `retryable`, mapeo de
        `command_execution` a `ToolUse`, sesión sin evento terminal,
        fallback del modelo a `"default"` cuando no se pidió ninguno,
        los tres modos de sandbox, `--model` como flag propio, `resume`
        con el `thread_id`, exterminio real del árbol de procesos (mismo
        test de nieto que `claude_code`), y las cuatro incorporadas junto
        con el mapeo ampliado: `file_change` → `ToolUse` con el primer
        `path` como digest, `mcp_tool_call` → `ToolUse` con
        `"{server}:{tool}"` como digest, `web_search` → `ToolUse` con la
        query como digest, y confirmación de que `reasoning` nunca se
        expone. Sin smoke test manual — ver arriba.

- [x] **T7.5 — `yunta stats <run_id>` y `--workflow X` (§8.4/§8.6, D77/D91).**
      `crates/engine/src/stats.rs` (pura, sin IO) deriva todo del log +
      workflow; `crates/cli/src/commands/stats.rs` hace el IO (leer
      storage/manifest) y renderiza.
      - **CPTV** (§8.4: tokens totales del run / tareas `done`) — la
        implementación que ya existía en `run/mod.rs` (usada para
        `RunFinished.metrics.cptv`) se movió tal cual a
        `stats::cptv(&RunState)`, para que los dos call sites nunca
        puedan desacordar.
      - **Tasa de re-trabajo**: tokens de todo attempt con `attempt > 1`
        (`NodeStartedPayload.attempt` ya distingue un primer intento de
        un reintento o de la corrección de una re-ruta) sobre tokens
        totales del run.
      - **Tasa de cache**: `cached_input_tokens` sobre input total —
        `None` (no "0%") si ningún adapter del run reportó la extensión
        opcional, distinto de `Some(0.0)` si reportó y dio cero.
      - **Costo por nodo y por rol**: sumado a través de todos los
        attempts de cada nodo (no solo el último, a diferencia de
        `NodeState` de `replay.rs`, que pisa con el attempt más
        reciente); por rol agrupa por el último `runner_resolved.role`
        visto para ese nodo. **Costo por modo no aplica dentro de un
        run** — un run tiene un solo `mode` (M9/`modes:` no existe
        todavía) — por eso vive en `--workflow`'s tabla comparativa, no
        en `stats <run_id>`.
      - **Tiempo de pared por nodo con fracción bloqueada**: `blocked`
        = gap entre que un nodo queda listo (máximo de los timestamps
        terminales de sus `depends_on`, o el primer evento del run si no
        tiene dependencias) y su primer `node_started`; `active` = suma
        de (terminal − started) de cada attempt. **Best-effort, no
        replay del scheduler** — documentado explícitamente en el propio
        doc del módulo: un hijo de `parallel` solo es tan preciso como
        su propio `depends_on` declarado, sin simular semántica de join.
        Es exactamente el insumo que A-08 pide (dato de wall-clock desde
        `stats` como gatillo), no una verdad absoluta.
      - **Visualización de terminal (D77)**: barras horizontales por
        nodo y por rol, sparkline (8 niveles Unicode) de CPTV histórico
        por workflow, tabla comparativa por modo — las tres, sin ANSI ni
        color en absoluto (no solo "degradable sin color": no hay color
        que degradar). Cada línea se construye con ancho fijo
        (`BAR_WIDTH=20`, `LABEL_WIDTH=12`) para caber en 80 columnas;
        verificado con un test end-to-end real contra el binario
        compilado, no solo por inspección.
      - **`--json`**: DTOs propios en el CLI (`RunStatsJson`,
        `WorkflowHistoryJson`, etc.) en vez de derivar `Serialize`
        directo sobre los tipos del engine — así la forma del JSON
        (duraciones en segundos `f64`, no la forma de
        `std::time::Duration`) es una decisión de presentación del CLI,
        no una fuga del tipo interno del engine.
      - **Estimación previa (§8.6/D91)**: mediana y p90 de tokens,
        wall-clock y cantidad de tareas sobre el historial de runs del
        mismo `workflow.name`, mostrada en `yunta run` (antes de crear
        el run, con el historial de runs *anteriores* — el run que se
        está por crear nunca se cuenta a sí mismo) y en `list_workflows`
        (una línea por workflow catalogado). Percentil por
        nearest-rank, determinista (sin interpolación) — necesario para
        que el golden test sea reproducible. **Menos de 3 runs → no
        dice nada en absoluto**, verificado con un test end-to-end que
        corre el mismo workflow 4 veces y confirma que las corridas 1–3
        no muestran estimación y la 4ª sí (con 3 corridas previas ya
        terminadas).
      - **`pricing:` (§8.4)**: campo nuevo en `ConfigLayer`
        (`{model: cost_per_1k_tokens}`) — no estaba en el recorte de
        T1.2, agregado ahora porque T7.5 es exactamente el consumidor
        que el propio comentario de módulo de `config.rs` pedía antes de
        construirlo. Sin `pricing:` declarado, todo queda en tokens y no
        se inventa nada (I20). Con `pricing:` declarado, se agrega una
        línea de estimado en moneda **además de** los tokens, nunca en
        su lugar.
      - **Advertencia de presupuesto vs. p90 (§8.6) ✓ (cerrada por
        DI-05)**: `limits:` entró a la config y `yunta run` compara
        `limits.max_tokens_per_run` contra `PriorEstimation.tokens.p90`
        (`yunta_engine::budget_p90_warning`, pura) — informativa, jamás
        bloqueante, y muda con <3 corridas o sin cap declarado.
      - **`pricing:` con más de un modelo priceado**: la línea de moneda
        promedia el costo-por-1k de todos los modelos declarados
        (`sum/count`), documentado en el propio código como una
        decisión explícita — no hay atribución de tokens a modelo
        específico expuesta en `RunStats` hoy, así que promediar es
        preferible a elegir arbitrariamente la primera entrada de un
        `HashMap`. Si el atributo por-modelo llega a necesitarse, es una
        extensión de `NodeStat`/`RunnerResolved` para rastrear qué
        modelo corrió cada nodo, no un cambio de `stats.rs`.
      - **Rendimiento de la verificación (§8.7/D93) — explícitamente
        fuera de esta tarea.** El Plan lo separa en T7.10, no T7.5;
        no se tocó nada de eso acá.
      - Tests: 11 en `crates/engine/tests/stats.rs` (golden, sobre un
        log fixture con un reintento y una re-ruta — construido a mano,
        mismo estilo que `tests/progress.rs`) cubriendo CPTV, tasa de
        re-trabajo, tasa de cache (con y sin reporte), tokens/attempts
        por nodo, tiempo bloqueado, agrupación por rol, wall-clock total,
        un nodo que nunca arrancó, estimación con <3 y con ≥3 runs, y que
        `run_summary` reusa `compute_run_stats`. 4 tests end-to-end en
        `crates/cli/tests/stats_cmd.rs` contra el binario real: ancho de
        80 columnas + ausencia de códigos ANSI, la floor de 3 runs para
        la estimación (en `run`, `list` y `stats --workflow` a la vez),
        `stats --workflow` sin runs, y el error de `stats` sin
        `run_id` ni `--workflow`.

- [x] **T7.6 — Onboarding: `yunta init` y `yunta new` (D58).** Antes de
      implementar se buscaron D58/D64/D74 en el propio doc de ADRs (no
      estaban citadas en ningún lugar del código todavía) para no
      inventar sobre un onboarding que la spec sí define con bastante
      detalle.
      - **`init`**: detecta ecosistema (`Cargo.toml`→rust,
        `package.json`→node, `go.mod`→go, `pyproject.toml`→python, en
        ese orden — primer match gana, documentado así porque un repo
        con dos marcadores sigue siendo "principalmente" el primero),
        rama base (`git symbolic-ref refs/remotes/origin/HEAD`, luego
        `git branch --show-current`, luego `"main"`), y CLIs
        disponibles vía `ClaudeCodeAdapter`/`CodexAdapter::probe()` —
        el mismo `probe()` real que `doctor`/`run` usan, con
        `AdapterSettings::default()` porque a esta altura todavía no
        hay `runners:`/`adapters:` en ningún config (eso es
        precisamente lo que `init` va a escribir). Escribe
        `.yunta/config.yaml` (con `project:` resuelto, y `runners:`
        como bloque **comentado** citando qué adapter se detectó —
        nunca un modelo inventado: T7.5 y el resto del código ya
        establecieron "jamás inventar" para nombres de modelo, y un
        `model: claude-...` adivinado sería exactamente ese error) y
        `.gitignore` (entradas defensivas — `paths.runs`/
        `paths.worktrees` ya viven en `~/.yunta` por default, D58's
        propio "lo personal en `~/.yunta/`", así que hoy no hay nada
        que gitignorear; las entradas cubren el día que alguien
        redirija `paths:` hacia el repo). Idempotente: rechaza
        pisar `.yunta/config.yaml`/la skill sin `--force`.
      - **Skill de mecanismo (D74)**: `.yunta/skills/yunta-mechanism/
        SKILL.md`, instalada siempre (no opcional — D74 solo hace
        opcional la línea de CLAUDE.md, no la skill), sin catálogo
        embebido (consulta `yunta list`/`list_workflows` en el
        momento, tal como D74 exige). **Formato `SKILL.md` con
        frontmatter — decisión de implementación, no algo que ningún
        doc especifique**: ni el ADR ni la página de "Config y
        workflows de referencia" fijan un formato de archivo para el
        contenido de una skill, solo que es un directorio montado por
        `skills.paths` "por el mecanismo nativo del adapter" — se
        adoptó el formato real de Claude Code (el adapter de
        referencia del proyecto, T7.3) por ser el precedente concreto
        más cercano, no una alternativa inventada sin apoyo.
      - **Línea de CLAUDE.md — solo impresa, jamás escrita** (D74 es
        explícito: "jamás escrita automáticamente"), ni siquiera
        cuando el repo ya tiene un CLAUDE.md — verificado con un test
        que crea un CLAUDE.md con contenido propio antes de correr
        `init` y confirma que sigue byte a byte igual después.
      - **D64 (cachés compartidas entre worktrees) — solo un tip
        impreso por ecosistema, nunca una clave de config nueva.** El
        ADR describe el patrón ("directorio de artefactos común,
        dependencias enlazadas, worktrees reutilizables") sin fijar
        una clave concreta en ningún schema de referencia — inventar
        una (`cache_dir:` o similar) sería exactamente el tipo de
        decisión de schema no pedida por ninguna tarea que CLAUDE.md
        pide evitar. `init` imprime un tip específico del ecosistema
        detectado (p. ej. `CARGO_TARGET_DIR` compartido para Rust) en
        vez de escribir algo que el resto del engine no sabría leer.
      - **`-i`/`--interactive`**: sin TTY, degrada avisando por stderr
        y sigue con los valores detectados — nunca cuelga esperando
        una línea que no va a llegar (mismo patrón que
        `ConsoleInteraction`, T7.2, reutilizado: `stdin().is_terminal()`).
        Con TTY, permite confirmar/editar nombre de proyecto y rama
        base.
      - **`new <name> [--shape one-node|lint-fix|ledger] [-i] [--force]`**:
        tres esqueletos comentados, "más cerca de `cargo new` que de un
        workflow real" (D58 verbatim) — **ninguno declara `runner:`**
        (queda comentado, `# runner: implementer  # uncomment...`)
        precisamente para que `check` nunca dependa de que
        `runners:` ya exista en el config local; verificado con un
        test que corre `new` en un repo donde `init` nunca corrió.
        `one-node` es un `bash` con su propio exit code como criterio
        más `scope:`; `lint-fix` es el ejemplo lint→fix-lint→lint del
        Contrato §11.2 (mismo shape que el fixture de T4.4 en
        `crates/engine/tests/run.rs`); `ledger` es el ciclo
        plan→loop del bootstrap (`the_bootstrap_shape_runs_end_to_end_
        plan_loop_and_gate`) despojado a su forma mínima. Corre
        `check` sobre lo que acaba de escribir y reporta el resultado,
        igual que `yunta check`. **`--shape` por default es `one-node`**
        cuando no se pasa ni `--shape` ni `-i` con TTY — no está fijado
        en ningún doc, elegido por ser el shape más simple posible (el
        equivalente de `cargo new`'s "Hello, world!").
      - **Nunca referencia un pack ni toca `yunta.lock`** (D58's regla
        de verbos disjuntos: `new` crea contenido propio, `pack add`
        —M11— trae contenido ajeno) — trivialmente cierto hoy (M-0 no
        tiene schema de packs), pero igual cubierto por un test
        estructural que falla si algún esqueleto futuro menciona
        `pack` o si `new` llega a crear `yunta.lock`.
      - Tests: 12 end-to-end en `crates/cli/tests/init_new_cmd.rs`
        contra el binario real — escritura de config/gitignore/skill,
        CLAUDE.md nunca tocado, idempotencia con y sin `--force` (init
        y new), degradación sin TTY para ambos comandos (sin colgarse),
        detección de ecosistema Rust, los tres shapes pasando `check`,
        el test estructural de packs/lock, shape desconocido rechazado
        sin escribir archivo, nombre inseguro rechazado, y `new` antes
        de que `init` haya corrido nunca.

- [x] **T7.7 — Gates externos por pull request (§5.6, D66).** La tarea
      más grande de M7 hasta ahora: introduce `kind: gate` al schema (no
      existía — T7.2 construyó el mecanismo de escalación §5.3 pero
      solo lo conectó a re-rutas agotadas, nunca a un node kind propio),
      un trait `Forge` nuevo en `yunta-adapters` (paralelo a `Adapter`,
      misma razón: frontera con un sistema externo, real + mock para
      testear sin red, A8 extendido a forjas), y la parte más delicada
      del diseño: qué pasa cuando una aprobación deja de cubrir el
      commit actual del PR.
      - **Schema**: `NodeKind::Gate { assignee, external: ExternalGate }`
        — `external` es **obligatorio, no `Option`**: un `kind: gate`
        sin forja detrás no está definido por ninguna tarea todavía (el
        caso interno ya lo cubre §5.3/T7.2 sin necesidad de este node
        kind). `ExternalGate { kind: ForgeKind, artifacts: Vec<String>,
        branch: String }`, `ForgeKind::PullRequest` como único variante
        (enum cerrado, no un string suelto — mismo criterio que
        `CheckBuiltin`). Nueva variable de template `{{run.branch}}`
        (`yunta/<run_id>`) — deliberadamente NO el branch local del
        worktree (`isolation: none` nunca crea uno, `worktree.rs`'s
        propio doc comment), sino un push target fresco.
      - **`forge:` en `ConfigLayer`** (`{github: {repo, token_env}}`) —
        no estaba en el recorte de T1.2 ni en ningún schema de
        referencia citado por D66/§5.6 (que hablan de "la forja" en
        abstracto); agregado ahora porque T7.7 es su consumidor real,
        mismo patrón que `pricing:` en T7.5. `token_env` nombra una env
        var, nunca el token (I12/O3, mismo convenio que
        `McpServerConfig::auth_env`).
      - **`check`**: `kind: gate` con `external.kind: pull_request`
        exige `forge.github` configurado (`CheckError::
        ExternalGateWithoutForge`) — el ✓ del plan, literal. Un gate
        **no puede ser hijo de `parallel`** (`CheckError::
        GateInsideParallel`) — su resolución es un round-trip a la
        forja, uno a la vez; T7.7 no define qué significaría para el
        join semantics de `parallel` compartir worktree con eso, así
        que se rechaza en vez de adivinar.
      - **`Forge` trait** (`yunta-adapters::forge`): `publish()` (commit
        de los artifacts declarados + apertura del PR, idempotente —
        busca un PR existente por branch antes de abrir uno nuevo) y
        `poll()` (devuelve `PolledGate{head_sha, review}` — el head
        actual del PR **separado** de `review`'s propio `reviewed_sha`,
        a propósito: comparar ambos es lo que decide si una aprobación
        sigue cubriendo el commit actual, y esa comparación vive una
        sola vez, en el engine, no duplicada entre `GitHubForge` y
        `MockForge`).
        - **`GitHubForge`**: REST v3 puro — `git/refs`+`contents` para
          publicar (sin git local: crea el branch y comitea archivos
          por HTTP), `pulls`/`pulls/.../reviews`/`pulls/.../comments`
          para el poll. Última revisión decisiva (`APPROVED`/
          `CHANGES_REQUESTED`) gana; `COMMENTED`/`DISMISSED` no son
          decisiones que §5.6 mapee a nada. **Sin smoke test contra la
          API real — mismo gap documentado que T7.4's `codex`**: sin
          token ni repo descartable en este sandbox. Los endpoints y
          shapes de campo son los reales y vigentes de la REST API de
          GitHub, no una adivinanza — pero eso no reemplaza correrlo.
        - **`MockForge`**: estado en memoria (`MockForgeState`,
          clonable/compartible vía `Arc<Mutex<_>>`) con métodos que un
          test llama DIRECTO, nunca a través del trait `Forge` —
          `approve`/`request_changes`/`close`/`push_commit` simulan
          exactamente "lo que pasa en la forja sin que Yunta esté
          instalado ahí", el punto central de D66.
      - **Dispatch**: un gate nunca pasa por `execute_node` (agregar un
        `NodeEnd::Waiting` al contrato compartido con
        bash/prompt/loop/check/executor habría sido un cambio mucho más
        invasivo que darle su propio par de `ScheduleStep`s puros —
        `PublishGate`/`PollGate`, uno a la vez, mismo criterio que
        `GateExhaustedReroutes` ya usaba). `schedule.rs` intercepta un
        gate listo en su sección 3 (antes de armar el batch genérico) y
        un gate `Running` (solo llega ahí por el recheck de abajo) en
        una sección 0 nueva, antes de la detección de huérfanos —
        `on_interrupt`'s `fail_if_uncertain`/`restart_node` no tienen
        sentido para algo que no es una sesión.
      - **Aprobado/cambios/cerrado (§5.6)** se resuelven reusando
        exactamente el vocabulario de eventos que cualquier otro nodo ya
        usa: aprobado → `node_finished` (el DAG sigue); cambios pedidos
        → cada comentario se postea como `finding_posted` (§4.1) y el
        nodo falla `retryable: true` — **la re-ruta declarada por
        `on_failure.goto` de §5.6 no necesitó código nuevo**, es
        exactamente el mecanismo de re-rutas que T4.4 ya construye;
        cerrado → falla `retryable: false`. `gate_resolved` ahora
        carga `approved_sha` (campo nuevo, junto a `resolved_by` que ya
        existía) — el timestamp ya viene gratis del envelope del evento
        — cumpliendo el "usuario+timestamp+SHA" del criterio.
      - **Detección de SHA post-aprobación (§5.6) — la pieza más
        sutil, con un límite explícito y deliberado.** Un
        `recheck_approved_gates` corre una vez por invocación de
        `execute_run` (antes del loop, nunca por iteración — "al
        despertar" es una vez por wake, no una vez por paso de
        scheduling), busca gates `Finished` cuyo `approved_sha`
        grabado ya no coincide con el head actual del PR, y los
        reabre emitiendo un `node_started` extra (mismo mecanismo que
        `restart_node` de T4.5 ya usa para huérfanos) para que el
        dispatch ordinario los vuelva a resolver desde cero.
        **Descubierto empíricamente, no diseñado de entrada**: el
        primer intento de este recheck vivía después del chequeo de
        "run ya `run_finished`" en `execute_run` — que ya existía antes
        de T7.7 y es correcto en general (un run terminado es
        inmutable, I2/§2) — así que un gate aprobado como ÚLTIMO nodo
        del run nunca llegaba a recheck-earse: el run ya había cerrado.
        Un test end-to-end (`a_commit_after_approval_returns_the_gate_
        to_waiting`) lo encontró en rojo antes de mover nada de código
        — exactamente el ciclo test-first que CLAUDE.md pide. La
        resolución, documentada en el propio código
        (`recheck_approved_gates`'s doc comment): el recheck **solo
        importa, y solo corre, mientras el run sigue abierto** — un
        workflow cuyo gate es su último nodo nunca se re-abre una vez
        aprobado (el run ya cerró, D66 no promete reabrir recibos
        emitidos); uno con trabajo pendiente después sí, en cada wake,
        hasta que ese trabajo también se resuelva. No es una limitación
        accidental — es la consecuencia directa de que `run_finished`
        sea de verdad terminal, que es exactamente lo que I2 exige.
      - **Degradación a consola sin forja/credenciales (D66)**: mismo
        objeto de escalación §5.3 que `GateExhaustedReroutes` ya usa
        (dos opciones, aprobar/rechazar), nunca publica nada — y, igual
        que ese código, no graba `gate_waiting` si `human_interaction`
        devuelve `None` (sin TTY): así un gate degradado sin resolver
        vuelve a preguntar en cada wake en vez de recordar una decisión
        que nunca se tomó de verdad.
      - **`--adapter`/CLI**: `real_forge(config)` en `commands/mod.rs`
        (mismo patrón que `real_adapters`) construye `GitHubForge` solo
        si `forge.github` está configurado Y la env var de
        `token_env` está seteada en el proceso — ausencia de lo segundo
        es exactamente el caso legítimo de la máquina de la persona B,
        no un error. `run`/`resume` pasan el forge del config
        correspondiente (el del manifest congelado en `resume`, nunca
        el del proyecto actual — mismo principio de "el run nunca
        relee config" que `adapters` ya sigue ahí). `yunta test` pasa
        `None` siempre (A8: sin forja real en fixtures mock).
      - **Deuda/límites explícitos**:
        1. Un gate dentro de `parallel` es rechazado, no soportado —
           semántica de join+worktree compartido con un round-trip a
           forja queda sin diseñar.
        2. Un solo forge (`GitHubForge`); `ForgeKind` cerrado a un
           variante deja el punto de extensión listo pero no hay
           segundo forge implementado.
        3. Consenso multi-reviewer no modelado: "última revisión
           decisiva gana", no una política configurable de cuántas
           aprobaciones hacen falta — §5.6 no lo pide y GitHub mismo
           delega eso a branch protection, fuera del alcance del
           engine.
        4. Los paths de `artifacts:` se interpretan relativos a
           `run.dir/artifacts/` y se comitean al mismo path relativo en
           el branch — convención razonable, no algo que ningún doc
           fije explícitamente.
        5. `GitHubForge` sin smoke test en vivo — mismo gatillo que
           T7.4: retomar cuando haya token+repo descartable
           disponibles.
      - Tests: 3 en `crates/engine/tests/check.rs` (gate sin forja
        rechazado, gate con forja aceptado, gate hijo de `parallel`
        rechazado) + 5 en `crates/engine/tests/external_gate.rs` contra
        `MockForge` — el escenario D66 completo (persona B aprueba sin
        Yunta, persona A lo recoge en un `execute_run` separado),
        commit posterior a la aprobación devuelve el gate a esperar (el
        propio test que encontró el bug del `run_finished` inmutable),
        cambios pedidos postea findings y falla retryable, PR cerrado
        falla no-retryable, y degradación a consola sin forja nunca
        publica nada.

- [x] **T7.10 — Rendimiento de la verificación (§8.7, D93). Cierra M7.**
      Módulo puro (`verification_effectiveness.rs`, mismo functional
      core que `stats.rs`) que deriva señales del histórico de runs de
      un workflow; `stats --workflow` y `yunta check` hacen el IO
      (juntar los logs crudos) y lo imprimen — nunca bloquea, nunca
      cambia el exit code de ninguno de los dos.
      - **Métrica núcleo: tasa de rojo en pre-check** — un criterio que
        nunca estuvo rojo ANTES del trabajo se marca; uno rojo-antes/
        verde-después, por más veces que corra, nunca aparece (test
        explícito para la distinción, tal como pide el ✓).
      - **Cuatro señales con evidencia**, cada una con su propio piso de
        `MIN_SAMPLES = 3` (igual al de la estimación de T7.5,
        deliberadamente — "sin distribución detrás es adivinanza"):
        criterio nunca rojo en pre-check (dos lecturas: redundante O
        mal escrito, mostradas juntas); re-ruta que nunca se disparó
        (**corregido durante el desarrollo**: la primera versión solo
        contaba una muestra cuando el nodo *fallaba* — pero un nodo que
        SIEMPRE termina limpio, sin fallar nunca, es exactamente el
        caso más fuerte de "el flujo previo es más confiable de lo
        previsto"; el smoke test manual lo mostró en blanco donde
        debía mostrar un hallazgo, se corrigió a contar toda ejecución
        del nodo, no solo sus fallos); gate siempre aprobado sin ajuste
        (`approved_sha` en `None` es la marca de "necesitó ajuste" —
        cubre tanto el "retry" interno de T7.2 como el
        changes-requested/closed externo de T7.7, sin lógica separada
        por mecanismo); tareas que siempre pasan al primer intento
        (agregado a nivel de **workflow, no por tarea** — el id de una
        tarea no es una identidad estable entre corridas con ledgers
        re-planeados distintos, así que "esta tarea exacta" no es una
        afirmación que el log pueda sostener entre runs; "cualquier
        tarea en cualquier run" sí).
      - **Evidencia por criterio/re-ruta/gate, nunca por workflow**: el
        piso de muestras se aplica a cada cmd/nodo/gate por separado —
        un criterio nuevo en un workflow con 50 corridas de historial
        arranca en cero muestras propias.
      - **Modos — deliberadamente sin implementar, no un olvido.**
        §8.7 pide dos señales atadas a `modes:` ("modo sin uso" y
        "nunca sugiere quitar nodos `invariant: true`") — `modes:`
        (M9) no existe en el schema de este codebase todavía. Inventar
        un schema de modos completo solo para que estas dos señales
        tengan algo que analizar hubiera sido exactamente la clase de
        decisión de diseño no pedida por ninguna tarea que CLAUDE.md
        pide evitar — M9 es su propio milestone, con su propia
        secuencia de dependencias (`M9 requiere M4+M5`, no M7).
        Documentado acá como gatillo: el día que `modes:` exista, estas
        dos señales se agregan a este mismo módulo. **[Resuelto después
        por DI-06, con T9.1 ya existente]**: señal `unused_modes` (modo
        declarado que ningún run del historial eligió, mismo piso de
        muestras) y el ✓ estructural — un nodo `invariant: true` queda
        excluido por construcción de `never_triggered_reroutes` y
        `always_approved_gates` (su re-ruta jamás disparada ES el nodo
        haciendo su trabajo, nunca un candidato a remoción).
      - **Las tres guardas del ✓**: (a) "sugiere, jamás actúa" — cierto
        por construcción de tipos, `analyze` toma `&Workflow` (nunca
        `&mut`) y devuelve hallazgos poseídos, no hay forma de que
        mute nada; (b) "nunca sugiere quitar `invariant: true`" — vacío
        hasta que exista `modes:` (ver arriba), no aplicable todavía;
        (c) evidencia por criterio — cubierta arriba.
      - **Superficies**: `yunta check` (stderr, junto a sus propios
        warnings — nunca cambia el exit code, verificado con test) y
        `yunta stats --workflow` (stdout, y en el DTO de `--json`).
        Ambas comparten exactamente el mismo texto renderizado
        (`render_verification_findings`, una sola función, nunca
        redactado dos veces).
      - Tests: 11 en `crates/engine/tests/verification_effectiveness.rs`
        (las cuatro señales, cada una con su caso positivo y negativo,
        el piso de muestras, la distinción rojo-antes/nunca-rojo, y sin
        historial no hay nada que decir) + 2 end-to-end en
        `crates/cli/tests/verification_effectiveness_cmd.rs` contra el
        binario real (el hallazgo aparece en `check` Y en `stats
        --workflow`/`--json` sin romper ninguno de los dos; menos de 3
        corridas no muestra nada).

## M9 — Modos, promoción y composición (completo: T9.1–T9.4)

- [x] **T9.1 — Modos abiertos (§10.1, D44).** `modes:` no existía en
      absoluto en el schema hasta ahora — el propio `Workflow` de M-0/M7
      llevaba una nota explícita ("minus modes, out of scope"). Agrega
      `modes: Option<IndexMap<String, ModeSpec>>` (mapa **ordenado** —
      `indexmap` nuevo como dependencia directa de `yunta-core`, ya
      presente transitivamente en el árbol vía otros crates, así que no
      crece el grafo real) y `invariant: bool` en `Node`.
      - **`include: all` vs. `include: [id, ...]`**: `ModeInclude` con
        `Serialize`/`Deserialize` escritos a mano en los dos sentidos —
        **encontrado en caliente, no en el diseño**: el primer intento
        derivaba `Serialize` sobre el enum `#[serde(untagged)]`, que
        emite la variante unitaria `All` como YAML `null`, no como el
        string `"all"` que el `Deserialize` (también a mano, porque
        serde no distingue solo-por-forma un string vs. una secuencia
        sin ayuda) espera de vuelta. Ningún test lo atrapó — todos
        construían el `Workflow` una sola vez desde YAML sin volver a
        escribirlo — hasta un smoke test manual contra el binario real
        (`yunta run` escribe `manifest.yaml`, `yunta status` lo vuelve a
        leer): `include: all` rompía ese round-trip. Corregido con un
        `impl Serialize` manual, y el test que debería haberlo atrapado
        desde el principio ahora vive en
        `crates/core/tests/workflow.rs`. Recordatorio concreto de por
        qué CLAUDE.md insiste en correr las cosas, no solo compilarlas.
      - **`check`**: tres reglas nuevas, independientes del nombre o
        cantidad de modos (D44 lo pide explícito) — referencia a un
        nodo inexistente en `include:`; nodo `invariant: true` ausente
        de algún modo declarado (`include: all` lo cumple trivialmente,
        nunca hay nada que chequear ahí); y la coherencia interna del
        modo (§10.1): un nodo incluido cuyo `on_failure.goto` apunta a
        un nodo excluido de ese mismo modo es error, con el mensaje
        nombrando las dos salidas (incluir el destino, o quitar la
        re-ruta) — literalmente el texto que §10.1 pide.
      - **Filtrado en el scheduler**: `next_step` (antes tomaba
        `&Workflow` completo) ahora recibe también el set de ids que el
        modo resuelto incluye (`None` = sin restricción — sin `modes:`
        declarado, o el modo resuelto es `include: all`) y filtra las
        cinco secciones que iteran `workflow.nodes` — incluida la
        detección de "todo terminado" (T4.1 propio: un nodo excluido
        nunca llega a ningún estado terminal, así que contarlo ahí
        habría dejado el run esperando para siempre algo que nunca iba
        a correr). **Descubierto trazando el propio ejemplo de
        referencia**, no inventado: el modo "quick" de
        `build-feature.yaml` incluye `implement`, que depende de
        `approve-plan` — excluido de "quick" — y `ship`, que depende de
        `fix-findings` — también excluido. La dependencia de un nodo
        incluido hacia uno excluido se trata como ya satisfecha
        (`deps_satisfied` en `schedule.rs`) — un modo recorta
        deliberación, nunca bloquea sobre lo que decidió saltarse.
      - **Congelado**: `create_run` ahora exige un `mode: &str` — el
        sentinel `"default"` nunca valida contra `modes:` y nunca
        filtra (equivalente a "sin restricción"), y cualquier otro
        nombre se valida contra los modos declarados del workflow
        *antes* de escribir nada (`RunError::UnknownMode`). El nombre
        elegido se graba en `run_created.mode` (campo que ya existía
        desde T2.2, sin consumidor hasta ahora) y nunca se vuelve a
        resolver — un resume lo lee del propio log, mismo criterio que
        la resolución de runners (§13.1) ya sigue.
      - **CLI**: `yunta run --mode <name>` ahora funciona de verdad (el
        flag existía desde T7.1, rechazado explícitamente hasta hoy).
        Omitido con `modes:` declarado por defecto usa el **primer**
        modo declarado — la promoción (§10.2) solo escala hacia
        adelante, así que arrancar en el piso es el único default que
        nunca necesita revertirse. `yunta test` (T7.9) siempre usa
        `"default"`: un caso de test no declara su propio modo, y
        ejercitar el grafo completo es más útil que recortar uno de
        entrada.
      - **Deuda/límites explícitos**: `include:` de un modo solo nombra
        nodos de **primer nivel** — nunca hijos de un `parallel` (el
        propio §10.1 nunca lo ejemplifica); "clasificación por nodo
        temprano + gate" (la frase literal de §10.1) se interpreta como
        la composición de T9.1+T9.2 (un modo "quick" cuyo propio nodo
        inicial escala vía promoción si el trabajo lo excede) y no como
        un tercer mecanismo nuevo — ninguna tarea del plan pide
        `message`/`options`/`on` en un `kind: gate` genérico, y el
        Contrato mismo nunca define esa forma normativamente (solo
        aparece, sin definición, en los workflows de referencia); T9.2
        (promoción) es quien realmente completa esa lectura, y queda
        para la próxima tarea del milestone, en orden. **[Resuelto
        después por DI-04]**: el gate interno existe — `external` pasó a
        `Option`, `message`/`options`/`on` en el schema (round-trip del
        fragmento `approve-plan` de referencia), `on:` re-rutea con la
        semántica §11.2 completa (el gate re-pregunta al volver, sin
        `max_reroutes`: cada vuelta la conduce un humano), opción no
        mapeada finaliza el gate con esa elección como outcome, `abort`
        lo agrega el engine (convención T7.2), y `check` valida `on` ⊆
        `options`, targets existentes y la coherencia de modos que T1.3
        pedía textualmente para "opción de gate".
      - Tests: 6 en `crates/core/tests/workflow.rs` (round-trip de
        `include: all` — la regresión que atrapó el bug de serialización
        —, round-trip de `include: [...]` con orden de declaración
        preservado, `invariant` por defecto en `false`, workflow sin
        `modes:` en absoluto) + 7 en `crates/engine/tests/check.rs` (las
        tres reglas nuevas, cada una con su caso positivo) + 4 en
        `crates/engine/tests/modes.rs` contra el engine real ("quick"
        salta el nodo excluido y aun así termina, "full" corre todo,
        `"default"` ignora los modos por completo, un nombre de modo
        desconocido se rechaza antes de crear el run).

- [x] **T9.2 — Promoción = run sucesor (§10.2, D22).** El único ✓ de la
      tarea es "cadena auditada en ambos logs" — el resto (disparador
      exacto, herencia de contexto) queda deliberadamente subespecificado
      en el propio Contrato, así que cada pieza se decidió con el
      criterio más chico y mejor evidenciado disponible, documentado acá
      en vez de adivinado en silencio.
      - **Disparador**: la tabla de eventos del Contrato dice
        `promotion_signaled` | emisor **engine** — nunca un nodo con una
        tool propia (esa vía no existe hasta M8/MCP). La única escalación
        hoy conectada a un `HumanInteraction` real es la de re-rutas
        agotadas (§5.3/T7.2) — extendida con una tercera opción,
        `promote`, ofrecida **solo** cuando `modes:` declara un modo
        posterior al actual (§10.1's declaration-order ladder,
        `next_mode_after` en `schedule.rs`); sin eso, la escalación sigue
        siendo exactamente `retry`/`abort`, sin cambio de comportamiento
        para todo workflow sin modos. La escalación equivalente de T5.11
        (`scope_expansion` agotado, `Decision::Escalate`) **sigue sin
        conectarse a un gate real** — deuda ya documentada en su propia
        entrada de T5.11, deliberadamente no tocada acá para no ensanchar
        el corte de esta tarea.
      - **Mecánica**: elegir `promote` emite `promotion_signaled`
        (`reason`, `evidence`, `suggested_mode`) y cierra el run con
        `run_finished{terminal_state: Promoted}` — nuevo variant, cierre
        definitivo, igual de inmutable que cualquier otro `run_finished`
        (I3). `execute_run` devuelve `RunTerminal::Promoted{suggested_mode}`
        y **no crea el sucesor por sí mismo** — necesitaría el checkout
        original (`cwd`) para preparar un worktree nuevo, algo que nunca
        recibe (solo un worktree ya preparado, §7.3). Crear y arrancar el
        sucesor es trabajo de la capa imperativa: `commands/promote.rs`
        (`drive_promotions`), llamado desde `run`/`resume` justo después
        de su propio `execute_run`, en un loop — acotado automáticamente,
        porque `modes:` es una escalera finita y estrictamente hacia
        adelante (como mucho `len(modes) - 1` promociones antes de
        quedarse sin modo siguiente).
      - **Identidad del sucesor, determinista, sin inyectar entropía en
        el engine**: `{run_id}-promoted` — derivado enteramente del
        propio id del padre (que ya viene de afuera), nunca de reloj ni
        de un generador nuevo. Su `base_commit` es el HEAD actual del
        worktree del padre en el momento de promover (no el
        `base_commit` original) — el trabajo ya avanzado se hereda, no
        se descarta.
      - **`create_run` gana `promoted_from: Option<&RunId>`** (mismo
        patrón de extensión posicional que ya usó `mode`, T9.1) —
        graba en `run_created.promoted_from`, campo scaffolded desde
        T2.2 y sin consumidor hasta ahora.
      - **Herencia de contexto — simplificada a propósito, no la
        general de §12.** §10.2 pide que el sucesor incluya "artifacts,
        ledger y findings del antecesor automáticamente". Un ledger
        (`kind: task-ledger`) y cualquier `kind: findings` son archivos
        bajo `artifacts/`, así que copiar el directorio completo del
        padre al del sucesor los cubre a los tres sin inventar un
        `ContextSource` nuevo. **Deliberadamente más angosto que el
        mecanismo general de "runs vinculados" que el propio §12
        describe** (referencia explícita por id a un run enlazado
        específico) — esa infraestructura es compartida con T9.3
        (`kind: workflow`, que la necesita igual), así que construirla
        genérica recién cuando T9.3 llegue evita una versión tirada que
        habría que rehacer. Un finding emitido por el engine (motivo:
        `finding_posted` sin artifact — p. ej. una ampliación de scope
        denegada) **ahora sí sobrevive (cerrado por DI-10)**: el cierre
        por promoción deriva del log un
        `artifacts/findings-inherited.yaml` (schema `kind: findings`,
        deduplicado por location + título normalizado, primera aparición
        gana) que la copia de directorio ya arrastra; sin findings no se
        escribe archivo.
      - **`isolation: none` bajo promoción**: nunca vuelve a pedir el
        lock de `cwd` (que el padre todavía tiene, sin liberar — solo se
        libera al terminar de verdad, `RunTerminal::Finished`) — el
        sucesor simplemente reutiliza el mismo checkout sin re-preparar
        nada.
      - Tests: 3 en `crates/engine/tests/promotion.rs` (la mitad del
        engine — `promote` ofrecido y cierra con `promotion_signaled` +
        `run_finished: promoted`; nunca ofrecido sin modo posterior;
        sin `HumanInteraction` real nunca promueve por su cuenta) + 1
        en `crates/cli/src/commands/promote.rs` (`#[cfg(test)]`, la
        única forma de ejercitar código `pub(crate)` de un crate sin
        `lib.rs` — la mitad de CLI: `drive_promotions` de punta a
        punta contra git real, sucesor creado y corrido, artifact real
        heredado, cadena auditada en el `run_created.promoted_from` del
        sucesor y el `promotion_signaled` del padre).

- [x] **T9.3 — `kind: workflow`: composición como runs vinculados
      (§12).** Cada sub-workflow es un **run completo** (run_id,
      manifest, event log y run.dir propios — jamás expansión inline):
      el padre emite `child_run_created` (con `child_workflow_hash`
      para que la identidad efectiva viva en el evento), corre el hijo
      con el mismo `execute_run` recursivo, y el estado terminal del
      hijo es el resultado del nodo (`child_run_finished` +
      `node_finished` cuyo `tokens_used` es el total derivado del hijo
      — así el `Usage` agrega hacia arriba por replay ordinario, sin
      campo derivado nuevo). Schema: `NodeKind::Workflow { use, inputs,
      isolation }` con `WorkflowIsolation { worktree (default) |
      inherit }` — enum propio, no `Isolation` (el comentario de la
      referencia: "`inherit` solo en nodos workflow");
      `release-cycle.yaml` es fixture de round-trip en `yunta-core`.
      Decisiones mecánicas fijadas acá (ningún doc las pinaba):
      - **Resolución de `use:`**: `.yunta/workflows/<name>.yaml` en el
        **árbol de trabajo del propio run padre** — el catálogo
        versionado del repo, el mismo que `list_workflows` describe.
        Error accionable nombrando el path exacto si falta; el hijo
        recién nacido pasa por `check()` estático antes de gastar nada.
      - **`child_run_id` determinista**: `<padre>-<nodo>` (ordinal `-N`
        si una re-ruta vuelve a correr el nodo), derivado contando los
        `child_run_created` previos del nodo en el log — cero entropía
        en el engine. El evento del padre se emite **antes** del
        `run_created` del hijo: un crash en la ventana deja una
        referencia colgante (hijo sin eventos) que el próximo resume
        supersede con ordinal nuevo, en vez de una re-creación chocando
        con su propia mitad.
      - **Aislamiento**: `worktree` → árbol propio en
        `worktrees_root/<child_id>`, rama `yunta/<child_id>`, base = el
        HEAD del árbol del padre (el hijo ve el trabajo ya commiteado
        del padre); manifest del hijo con `paths` congelados (DI-07).
        `inherit` → el hijo opera directo en el árbol del padre y su
        manifest dice `isolation: none`: el engine jamás limpia,
        commitea ni lockea un árbol que no es suyo (un
        `on_finish.cleanup` del hijo NO puede borrar el árbol del
        padre por construcción).
      - **Presupuestos en cascada**: al nacer, el
        `limits.max_tokens_per_run` congelado del hijo = lo que le
        queda al padre (`cap - gastado`) — auditable en el manifest del
        hijo, y la maquinaria ordinaria de presupuesto del hijo aplica
        el techo del padre a todo el subárbol. Un `continue` humano en
        el padre (budget lifted) deja el cap propio del hijo intacto en
        vez de congelar un hijo ilimitado para siempre.
      - **Hijo pausado = padre en `waiting`** (§12: "un run padre que
        pasa la mayor parte de su vida en waiting"): `NodeEnd::
        ChildPaused` — sin evento terminal para el nodo (queda
        `running` huérfano) y el padre pausa nombrando al hijo; el
        resume del padre re-entra al nodo, encuentra el último
        `child_run_created` sin `child_run_finished` y **resume el hijo
        recursivamente** desde su propio manifest congelado. Cancelación
        raíz → `Interrupted` (huérfano, DI-11); carrera `join: any` →
        derrota registrada. En `join: any`, un hijo pausado no gana ni
        pierde: si un hermano gana, el grupo cierra y el run hijo queda
        pausado en su propio log (resumible con `yunta resume
        <child>`); sin ganador, el run pausa y el resume re-entra.
      - **Grupos `parallel` re-entrantes sobre huérfanos**: un hijo de
        grupo `running` sin evento terminal (crash, cancelación, o el
        nodo workflow abierto de arriba) ahora **se re-corre** al
        restart del grupo (§8.1 `restart_node` aplicado dentro del
        grupo, intento+1) — antes el filtro `to_run` lo saltaba y un
        resume podía cerrar el grupo sin re-correr al interrumpido.
      - **`check`**: `WorkflowNodeRunnerBinding` (`runner`/`runners`/
        `agent` sobre `kind: workflow` = aceptado-e-ignorado, A6);
        `InheritChildWithoutScope` (§12: hijos paralelos `inherit`
        exigen scope declarado para que la disyunción sea verificable —
        el solapamiento declarado ya lo cazaba
        `OverlappingParallelScope`); y `check_workflow_refs` (entry
        point separado porque `check()` no lee archivos): grafo de
        referencias resuelto contra el catálogo, `WorkflowRefMissing`/
        `WorkflowRefUnparseable`/`WorkflowRefCycle` (cadena `a -> b ->
        a`)/`WorkflowRefTooDeep` contra
        `resolved_max_workflow_depth()` (default de referencia: 4) —
        cableado en `yunta check` y `yunta run`; el mismo límite se
        re-aplica en runtime al nacer cada hijo (`RunCtx.depth`),
        porque los archivos pueden cambiar entre check y nacimiento.
      - **Modo del hijo**: primer modo declarado (el piso — la misma
        convención del CLI) o `default` sin `modes:`.
      - **Reproducibilidad histórica (✓ del plan)**: test que evoluciona
        el workflow hijo entre dos runs padre — el segundo congela v2,
        y el `child_run_id` del primero sigue apuntando a un manifest
        que hashea v1: el histórico jamás re-resuelve
        `nombre@versión-actual`.
      - **Deltas registrados**: DI-25 (cadena de promoción de un hijo —
        cerrado después: el padre crea y corre el sucesor
        automáticamente vía `create_promotion_successor`, compartido con
        el CLI, y la contabilidad pasó a `child_run_finished.tokens`
        para que cada miembro de cadena cuente exacto una vez) y DI-26
        (montaje cross-run declarativo de artifacts — cerrado después
        con ADR D108: `mounts: [{artifact: {node, name, as?}}]` lo
        declara el **padre** en su nodo `kind: workflow`, cada mount
        implica `depends_on`, y la entrega es copia al `artifacts/` del
        hijo al nacer — el mecanismo de la herencia por promoción
        generalizado; un hermano `kind: workflow` resuelve vía su último
        `child_run_finished`, cualquier otro nodo vía el `artifacts/`
        del propio padre; fuente faltante = `node_failed` antes de que
        exista el vínculo. El hijo consume con `artifact: {name}` sin
        `node` en `context:` — sigue sin saber que es hijo — y los
        `inputs:` quedan como canal de escalares y paths). Los runs
        hijos no cuentan contra
        `max_concurrent_runs` (ese cap gobierna invocaciones de `yunta
        run`, no el tamaño del árbol).
      - Tests: 7 en `crates/engine/tests/workflow_compose.rs` (run
        vinculado completo con inputs rendereados y árbol propio;
        agregación de tokens; pausa+resume recursivo vía gate interno;
        pinning histórico v1/v2; **release-cycle de referencia con mock
        de punta a punta** — 4 hijos, gates aprobados por interacción
        scriptada; techo de profundidad en runtime; archivo faltante
        nombrando el path), 7 en `tests/check.rs` (bindings de runner,
        scopes de `inherit`, grafo: faltante/ciclo/profundidad/sano) y
        2 CLI en `tests/run_flow.rs` (`yunta run` compuesto de punta a
        punta con hijo real bajo `YUNTA_HOME`; `yunta check` rechazando
        un ciclo del catálogo).

- [x] **T9.4 — Fan-out `runners:` (§13.2) + `agent:` a nivel nodo
      (§13.3, D37).** Expansión **estática en el manifest**
      (`expand_runner_fanout`, antes de las dependencias implícitas):
      un nodo con `runners: [a, b]` se vuelve `<id>@a` y `<id>@b`, cada
      uno con su `runner:` propio — visibles en `status` sin que el
      scheduler sepa nada de fan-out. Todo lo que referenciaba el id
      original sigue la expansión: `depends_on` aguas abajo se recablea
      a todos los hermanos y las listas `include:` de modos los nombran
      a todos. Re-rutas, `on:` de gates y referencias de contexto
      `artifact:` sobre un nodo fan-out son error de `check`
      (`FanOutTarget` — no hay "volver a review" inequívoco cuando
      review son varios nodos); `runner:` + `runners:` juntos también
      (`BothRunnerAndRunners`). Los nombres de artifacts se re-renderizan
      por nodo (`findings-{{runner.role}}.yaml` produce un archivo por
      hermano). `agent:` a nivel nodo pisa el del candidato del runner;
      un adapter sin `custom_agents` falla el nodo con mensaje accionable
      en vez de ignorarlo (A6). El fixture de referencia
      `build-feature.yaml` ahora usa el `runners: [reviewer,
      reviewer-alt]` literal de la doc — su último delta marcado quedó
      cerrado.

## M8 — MCP (completo: T8.1–T8.2; wiring de adapters reales pendiente vía DI-22)

- [x] **T8.1 — `yunta mcp`: superficie de control por stdio (§6.4).**
      `rmcp` ya era dependencia (T6.2, cliente saliente para `context:
      mcp`) — sus features de servidor pasaron de dev-only a reales en
      `yunta-cli` (`server` + `transport-io`). Cinco tools, servidor
      manual (sin macros/`schemars` derive — mismo estilo que el server
      de juguete de `mcp_context.rs`), cada uno con schema JSON y
      descripción **escrita para decidir** (D74): `list_workflows`,
      `run_workflow`, `workflow_status`, `resume_run`, `resolve_gate`.
      Config JSON de referencia para invocarlo:
      ```json
      { "mcpServers": { "yunta": { "command": "yunta", "args": ["mcp"] } } }
      ```
      **Nunca bloquea (I25/§6.4)**: `run_workflow`/`resume_run` disparan
      el mismo mecanismo de `--detach` y retornan de inmediato;
      `workflow_status` es la única vía de seguimiento (pull, jamás
      push — mismo modelo que los gates externos, D66/D101).
      - **`yunta run --detach`** (prerrequisito, nuevo): crea el run
        sincrónico (rápido, sin IO de agente) y lanza `yunta resume
        <run_id>` como hijo en su propio process group
        (`process_group(0)`, unix) — una señal al grupo del lanzador (el
        Ctrl-C de una shell) nunca lo alcanza; stdout/stderr van a
        `run.dir/scratch/detached.log`. Factoreado a
        `commands::spawn_detached_resume`, compartido con
        `resolve-gate`/`resolve_gate` (el tool) y `resume_run` (el
        tool) — cero mecanismo de ejecución nuevo, el hijo es un resume
        ordinario.
      - **`current_escalation(manifest, events)`** (prerrequisito,
        motor): reconstruye el objeto §5.3 de un gate sin superficie
        viva **puramente desde el log** — hoy, cuando `resolve()`
        devuelve `None`, el engine solo logueaba el `summary` como
        texto de pausa y descartaba `options`/`tradeoffs`/`evidence`, y
        no había forma de reconstruirlos después. `schedule::next_step`
        es puro, así que llamarlo de nuevo sobre el mismo log alcanza
        determinísticamente el mismo paso — las dos construcciones de
        escalación (antes duplicadas entre `run/mod.rs` y
        `gate_exec.rs`) se compartieron a un solo sitio cada una.
      - **`resolve_gate(manifest, storage, run_id, clock, option, by,
        text)`** (motor, puro sobre log): valida la opción elegida y
        apéndica `gate_waiting`+`gate_resolved`(+`NodeRerouted` en
        retry) — un `resume` ordinario, en cualquier proceso, drena la
        consecuencia. **Recorte deliberado**: solo el menú de un
        re-route agotado (`retry`/`abort`); `promote` necesita proceso
        vivo (distill+sucesor) y un gate interno sin resolver tiene una
        cadena de consecuencia más larga (`node_started`, y en opción
        no mapeada `node_finished`+reescritura de `progress.md`) —
        replicarla apurado arriesgaba una segunda copia de
        `gate_exec::resolve_internal_gate` que se desalinea. Ese recorte
        se registró como DI-27 y quedó **cerrado** con la arquitectura
        de **decisiones pre-sembradas**: `resolve_gate` escribe SOLO el
        par §5.3 (el `node_rerouted` duplicado se borró), y el engine
        que despierta consume la decisión por su único camino de
        consecuencia (`pre_seeded_resolution`, pura sobre el log:
        califica sii seq > último `node_failed`/`node_rerouted`/
        `node_finished` del nodo y > último `run_paused`) — retry,
        abort, `promote` y gates internos, uniformes, con la propiedad
        vivo-vs-presembrado verificada por test. Precondición nueva:
        `NotPaused`. Subcomando CLI: `yunta resolve-gate <run_id>
        <option> [--by] [--text]`.
      - **`list_workflows`/`workflow_status`** (el tool): shellean a
        `yunta list`/`yunta status` sobre este mismo binario en vez de
        duplicar su renderizado (ya es el texto estructurado D45) —
        una segunda copia acá se desalinearía con la real.
        `run_workflow` resuelve el nombre de catálogo contra
        `.yunta/workflows/<name>.yaml` (misma convención que `use:` de
        T9.3) y shellea a `yunta run <path> --detach`. `resume_run`/
        `resolve_gate` (el tool) llaman los primitivos ya factoreados
        directo en proceso, sin subprocess de más.
      - ✓ **Criterios cubiertos** (test E2E real: cliente `rmcp`
        hablando JSON-RPC por stdio contra el binario real, spawneado
        como proceso hijo — mismo patrón que `rmcp` documenta para
        testear un server stdio desde afuera): `list_workflows` refleja
        el catálogo sin regenerar nada; `run_workflow` retorna en
        milisegundos con un workflow que tarda segundos; **matar
        `yunta mcp` (SIGKILL, no cierre prolijo) y una sesión MCP nueva
        confirma vía `workflow_status` que el run siguió y terminó**
        (el criterio explícito del plan); `resolve_gate` responde un
        re-route agotado creado por otro proceso.
      - Tests: 7 en `crates/engine/tests/escalation.rs`
        (`current_escalation`/`resolve_gate`, reconstrucción +
        resolución + rechazos), 5 en `crates/cli/tests/run_flow.rs`
        (`--detach` × 2, `resolve-gate` × 2 vía CLI, más el flujo
        base), 3 en `crates/cli/tests/mcp_flow.rs` (E2E stdio, el de
        supervivencia al SIGKILL incluido).
      - **Flake preexistente detectado**: `run.rs`'s
        `eight_independent_tasks_at_concurrency_4_match_concurrency_1_state_and_commits`
        (T5.10) fallaba intermitentemente bajo `cargo test --workspace` —
        era un bug de producto (race de `git worktree add` concurrente),
        registrado como DI-28 y **cerrado**: toda mutación `git worktree`
        pasa ahora por `yunta-worktree.lock` en el common git dir
        (patrón DI-08, robo por borrado+`create_new` atómico).
- [x] **T8.2 — MCP por-run (§6.4/§6.5, D49/D98/D103/D104).** En tres
      cortes (a/b/c), completo salvo el wiring del adapter real (abajo).
      - **Schema (a)**: `coordination: independent | blackboard` en
        `NodeKind::Parallel` (D49, default `independent` — los grupos
        evaluativos jamás se ven entre sí salvo opt-in);
        `SessionRequest.run_tools_endpoint: Option<RunToolsEndpoint
        {url, token}>` en `yunta-adapters`; el mock registra
        `endpoints_seen` (mismo principio que `skills_seen`).
      - **Listener (b)**: `run_tools.rs` — un servidor MCP HTTP
        loopback **por sesión de nodo** (D103): puerto efímero, token
        bearer de un solo uso (2×uuid v4, jamás al log — I12), muere
        con su `RunToolsSession` (Drop cancela+aborta; un resume emite
        credencial nueva siempre). Scoping por construcción (I27): el
        listener sostiene su `(run_id, node_id, task)` y ninguna tool
        toma run_id del llamador. Los datos viven en storage
        (`Storage::reopen()` nuevo + `busy_timeout` 5s), nunca en el
        listener. Los 4 tools: `yunta_post_finding` (schema §4.1, el
        mismo tipo `Finding` de la vía artifact — D80; reporte
        incompleto = error visible nombrando el campo, no-op en el
        log), `yunta_task_status` (vista read-only derivada del log),
        `yunta_request_scope_expansion` (solo sesiones de tarea —
        §6.2 es task-keyed; valida el objeto idéntico y escribe el
        MISMO request file que la evaluación post-attempt existente
        consume: un mecanismo, dos superficies de entrada — round-trip
        probado contra el `load_request` real), `yunta_get_blackboard`
        (montada SOLO en grupos blackboard — D49 — y sirve
        exclusivamente los posts PROPIOS mientras el grupo corre —
        §5.9/D98).
      - **Wiring (c)**: `RunCtx.run_tools_host` (host por invocación;
        reopen fallido degrada con warn); sesiones prompt y tareas de
        loop abren listener fresco por intento cuando el adapter
        declara `run_tools` (`SessionSetup.run_tools` para el ciclo de
        tareas); capacidad ausente = endpoint `None`, el estado de
        reposo de §6.5 — NUNCA un evento de degradación... salvo que
        el nodo esté en un grupo `blackboard`, cuya coordinación
        declarada el engine jamás emula (A6): fallo de nodo con
        diagnóstico nombrando capacidad y coordinación. Consolidación
        D98 al cierre terminal del grupo (éxito o fallo):
        `consolidate_blackboard` (pura, ordena por contenido — jamás
        por orden de llegada) escrita como node-output del grupo,
        consumible por un nodo posterior
        (`context: [{node-output: {node: <grupo>}}]`), jamás entre
        hermanos en caliente. El mock ganó el paso de fixture
        `run_tool`: actúa como cliente MCP REAL contra el listener por
        streamable-HTTP (A8 — todo el camino en CI sin LLM; un
        `run_tool` sin endpoint o con error de tool falla la sesión
        ruidosamente).
      - ✓ cubiertos: dos sesiones concurrentes postean 16 findings sin
        pérdida ni mala atribución; nodo de grupo `independent` no ve
        las tools (ni listadas ni llamables); pre-join solo posts
        propios (con hermano y nodo ajeno ya posteados); consolidado
        byte-idéntico con órdenes de llegada invertidos (E2E con
        `after_ms` cruzados + test puro con shuffle); auth: token
        equivocado rechazado antes de cualquier tool; endpoint muere
        con la sesión. Tests: 9 en `tests/run_tools.rs` (cliente rmcp
        real), 7 en `tests/blackboard.rs` (E2E con mock posteando por
        el wire), 1 en storage (`reopen` intercalado).
      - **Fuera de este corte, explícito**: el wiring de `claude-code`/
        `codex` para traducir el endpoint al mecanismo nativo de MCP
        externo de cada CLI — ambos declaran `run_tools: false` hoy,
        y la degradación es exactamente la de §6.5 (sin capacidad, sin
        endpoint). Ese wiring pertenece a la familia de superficies
        construidas-contra-doc que la smoke-checklist (DI-22) ya
        cubre: cuando se cablee, entra con su paso en vivo propio.
        `timestamp` de eventos de tool y el token usan wall-clock/
        entropía directamente en `run_tools.rs` — decisión de borde
        documentada en el módulo (es la orilla más externa de la
        cáscara imperativa; nada puro los consume: replay lee
        timestamps almacenados).

## Decisiones de recorte explícitas (qué quedó afuera y por qué)

- **T1.1**: nodos `prompt`/`bash`/`loop`, más `parallel` desde T4.6 y `check`
  desde T5.4. Sin `gate`/`executor`/`workflow`; sin `context:`, `skills:`,
  `modes:`, `inputs:` (este último llegó después, vía T1.5 — ver la sección
  M7 más abajo), fan-out de `runners:`, `agent:` a nivel nodo,
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
5. **Event kinds sin consumidor en replay (T2.3)**: `questions_answered`,
   `finding_posted`, `child_run_*`, ampliación de scope, `promotion_signaled`
   — existen como tipos (T2.2) pero `derive()` los ignora porque nada los
   emite todavía. Sumarlos a `RunState` cuando su schema/ciclo llegue
   (composición: M9; findings: T5.12). **`gate_waiting`/`gate_resolved` y
   `questions_answered` — resuelto por DI-03**: `derive()` deriva
   `NodeState::Waiting{external_ref}` (§3.2) para un gate publicado sin
   resolución y para un nodo cuyo artifact `kind: questions` no tiene
   `questions_answered` posterior (el evento `artifact_written` ganó
   `artifact_kind`, aditivo D70 — nombrado así y no `kind` porque el
   envelope internally-tagged ya reclama esa clave en el JSON, colisión
   encontrada por un test e2e, no adivinada); el par interno de T7.2
   (waiting+resolved juntos) restaura el estado previo por construcción.
   El scheduler dejó de escanear eventos crudos (`was_published` borrado)
   y decide sobre el estado derivado; `status` muestra `waiting`
   distinguido y los nodos excluidos por modo como `skipped` (§8.5/D45,
   denominador visible y atribuible). El re-ask de questions en resume
   vive en `ScheduleStep::AskQuestions` (`questions_exec.rs`) — el ÚNICO
   sitio de ask, primera corrida y resume por el mismo camino.
6. **T1.5 — hecho.** Ver la sección M7 más abajo. El gatillo previsto acá
   ("cuando algo necesite inputs reales") terminó siendo `--input` de T7.1,
   no el workflow de bootstrap.
7. **`event_hash` (T2.5) ✓**: implementado según la política de
   `docs/eventos.md` §3 — hash encadenado por evento (columna
   `event_hash`, calculado en la misma transacción que asigna `seq`,
   campos length-prefixed en el orden fijo del schema), génesis
   `H0 = SHA-256(manifest_hash)` (el primer evento de un run DEBE ser
   `run_created` — `append_event` lo rechaza con `GenesisMissing` si
   no), y verificación explícita vía `yunta verify <run_id>`
   (`Storage::verify_chain`: payload alterado, evento
   borrado/insertado/reordenado, hash alterado o ausente → `Broken` con
   el seq exacto). Deuda menor: la política dice que la verificación
   "corre automáticamente al generar el recibo" — el recibo (§8.4) no
   existe todavía; cuando exista, llama a `verify_chain` antes de
   emitirse. DBs pre-T2.5 migran con `ALTER TABLE` y sus filas viejas
   quedan `NULL` (reportadas como ruptura, jamás backfilled).
8. **El arco que cerraba M-0 está COMPLETO** (los seis pasos: artifacts al
   cierre, templates mínimos, scheduler T4.1, creación de run, CLI
   run/status/resume, `yunta test`). Ver la lista de alcance arriba. Deudas
   menores que dejó, con su gatillo:
   - **T4.2 (worktrees) — hecho**, ver detalle en la sección "M4" abajo.
   - **T2.4 (paths congelados) ✓ (cerrado por DI-07)**: el manifest congela
     `paths: { runs_root, worktrees_root }` absolutos al crear el run;
     `resume`/`status`/`stats`/`cancel` buscan el run.dir en orden (paths
     actuales → default del user root) y de ahí en más todo sale de los
     paths congelados. Manifest pre-DI-07 (sin `paths:`) sigue resumible
     con el fallback a la config actual (lector tolerante, D70;
     `schema_version` del manifest 1 → 2). Límite documentado: un run
     creado bajo roots que ya no figuran en ninguna capa requiere
     `YUNTA_HOME` apuntando ahí (un índice global sería estado derivado
     como fuente de verdad, I2).
   - **Eventos `agent_session_opened`/`agent_message` ✓ (cerrado por
     DI-09)**: `dispatch_session` los emite a medida que llega el stream
     (un `status` concurrente ve la sesión viva) vía el trait
     `SessionObserver` que `RunCtx` implementa. `agent_session_opened`
     lleva session_id/agent/model/capabilities (prerequisito de DI-23
     `resume_session`); `agent_message` es acotado: tool_use →
     nombre+digest, usage → tokens, note → resumen mecánico
     `N bytes, sha256 <prefijo>` — jamás contenido (I12/O3, con test de
     redacción). `derive()` los ignora para el estado de nodos.
   - **`node-output` como artifact (§11.2)**: el stderr de un `bash` fallido va
     hoy en el diagnóstico de `node_failed` (acotado a 20 líneas), no como
     artifact montable por `context:` — eso es de T4.4/M6.
   - **Subgrafo de corrección**: la re-ruta ejecuta solo el nodo destino; "su
     subgrafo" completo llega con T4.4.
   - **`run_paused` por límites de presupuesto de run (§8.3) ✓ (cerrado por
     DI-05)**: `limits:` entró a la config; `max_tokens_per_run` escala §5.3
     (continue/abort, autorización por invocación) antes de cada paso que
     gasta tokens y degrada a `run_paused { reason: budget… }` sin
     superficie; `max_loop_iterations` (default de referencia 12) escala
     igual desde el loop; cada sesión recibe `Budget.max_tokens =
     min(restante, cap / nodos_no_terminales)`.
9. **Canal cross-process para `yunta cancel` (T7.1/A4) ✓ (cerrado por
   DI-08)**: `run.dir/scratch/engine.json` registra el pid del engine y
   los process groups vivos (sesiones vía `AgentSession::pgid()`, hooks,
   bash y executors vía guard RAII); Ctrl-C dispara el
   `CancellationToken` raíz → interrupt→kill ya construido → `run_paused
   { reason: "cancelled by user" }`, lock de `none` liberado, engine.json
   borrado; `yunta cancel` con engine vivo manda SIGINT y espera el
   terminal en el log (escalación SIGKILL a los pgids con timeout), y con
   engine muerto mata los pgids huérfanos, emite `run_paused { reason:
   "cancelled after crash" }` y limpia. `--detach` (M8/D101) consumirá el
   mismo engine.json. La deuda que seguía (sesiones de loop no
   cancel-aware) la cerró DI-11: el token raíz llega hasta cada
   dispatch, y la cancelación del usuario deja nodos huérfanos que el
   resume re-trata por `on_interrupt` en vez de `node_failed`
   fabricados que lo dejarían sin salida.
10. **Retención a nivel de base de datos ✓ (cerrado por DI-14)**:
    `Storage::purge_run(run_id)` (un método, no un query language — D53)
    y `gc` con orden de muerte explícito: primero `run.dir` (cuyo
    `events.jsonl` exportado es la copia autocontenida), y las filas
    recién en una corrida posterior, solo para runs cuyo run.dir ya no
    existe — la DB jamás es la primera copia en morir. Un run no
    terminal (pausado incluido) jamás se purga; un run purgado se lee
    como "unknown", nunca como estado corrupto; `--dry-run` reporta
    ambas fases.
11. **T7.4's smoke test manual, sin correr** — el criterio de aceptación
    lo pide explícito y este sandbox no tiene el binario `codex` ni
    credenciales de OpenAI para cumplirlo (a diferencia de T7.3, que sí
    tuvo `claude` real disponible). El mapeo de sandbox y el parser
    quedan construidos desde documentación y ejemplos de corridas reales
    confirmados, con la misma rigurosidad que T7.3 usó, pero sin el paso
    de confirmación en vivo. Gatillo: correr el smoke test descrito en
    `docs/m0-status.md`'s propia entrada de T7.4 (workflow de 3 nodos,
    igual al de T7.3, contra el `codex` real) la próxima vez que exista
    un entorno con el binario y credenciales — y si algo del mapeo de
    sandbox o del parser resulta incorrecto, corregirlo ahí, no
    silenciosamente al pasar por otra tarea.
12. **T7.5's advertencia de presupuesto vs. p90 histórico (§8.6), sin
    implementar** — mismo gatillo que el ítem 8's `limits.max_tokens_per_run`:
    no existe ningún campo de presupuesto declarable en el schema todavía
    (`Budget` es un tipo del trait Adapter que `node_exec.rs` construye
    siempre en blanco). El resto de §8.6 (mediana/p90 de tokens,
    wall-clock y tareas, mostrado en `yunta run`/`list_workflows`, mudo
    con menos de 3 runs) está completo. Gatillo: el mismo que el ítem 8 —
    la tarea que introduzca `limits:` en la config.
