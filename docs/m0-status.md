# M-0 — Estado de implementación

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
- [x] **T7.1** (parcial) — `yunta check <workflow> [--config <config>]` real en el
      CLI, lee YAML de disco. `run`/`status`/`resume` **siguen sin conectar — hay
      preguntas de diseño abiertas, ver más abajo, antes de seguir ahí.**
- [ ] **T7.3** — adapter `claude-code` real (no ejercitable en este sandbox: necesita
      el binario `claude` instalado y autenticado)

## Hecho de más, no nombrado explícitamente en el alcance mínimo

- [x] **T0.2** — CI (`fmt` + `clippy -D warnings` + build/test, `.github/workflows/ci.yml`).
      Pendiente confirmar que corre verde en GitHub real (no verificado desde acá).
- [x] **T1.2** (recorte) — config en capas: `runners`/`adapters`/`storage`/`paths`.
- [x] **T1.3** (recorte) — `yunta check` como función pura en `yunta-engine`.

## Decisiones de recorte explícitas (qué quedó afuera y por qué)

- **T1.1**: solo nodos `prompt`/`bash`/`loop`. Sin `gate`/`check`/`parallel`/
  `executor`/`workflow`; sin `context:`, `skills:`, `modes:`, `inputs:`, fan-out de
  `runners:`, `agent:` a nivel nodo, `permissions:`, `scope_expansion:`,
  `coordination:`. Confirmado con el usuario.
- **T1.2**: solo `runners`/`adapters`/`storage`/`paths`. Sin `mcp_servers`, `skills`,
  `baseline`/`coverage`, `secrets`, `permissions` (con su merge invertido, D51).
  Confirmado con el usuario.
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

## ⚠️ Decisión pendiente tuya antes de seguir con `yunta run`

No la resolví sola porque son decisiones de diseño genuinamente nuevas, no
recortes de algo ya especificado — inventar acá sería exactamente lo que CLAUDE.md
pide no hacer ("ante ambigüedad: ADRs → preguntar, jamás inventar"):

1. **¿Cómo encuentra un nodo `loop` su ledger?** M-0 dice explícitamente "durante
   el bootstrap no hay nodo de planificación: las tareas del plan se convierten a
   ledger a mano" — o sea, el ledger ya existe como archivo antes de `yunta run`.
   Pero el schema de `loop` (T1.1) no tiene un campo que apunte a ese archivo. ¿Se
   agrega un campo (`ledger: <path>`) al nodo, se usa una convención de path fija
   (`.yunta/ledger.yaml`), o algo distinto?
2. **¿Cómo sabe una invocación real de `yunta run --adapter mock` qué fixture usar
   para cada nodo `prompt`/`loop`?** `MockAdapter` (T3.2) necesita un fixture YAML
   por sesión — perfecto para tests donde el test lo construye a mano, pero
   `yunta run` desde la terminal no tiene de dónde sacarlo todavía. ¿Es esto
   territorio de `yunta test` (T7.9, con casos en `.yunta/tests/`) y `yunta run`
   con mock simplemente no es un modo pensado para invocación manual — o falta
   diseñar un mecanismo de fixtures por nodo?
3. **T1.4 (manifest)**: sin él no hay `manifest_hash` para `run_created`
   (`docs/eventos.md` §5.1). ¿Construyo una versión mínima ahora (solo lo que M-0
   necesita: hash del workflow, config, commit base — sin `modo`/`inputs`, que no
   existen) o esperamos a que haga falta con más contexto tuyo?

Mientras tanto seguí con todo lo que no dependa de esto — quedan varios ítems
abajo que sí son seguros.

## Pendiente explícito (para retomar sin adivinar)

1. **T7.3** — adapter `claude-code` real: probe (binario, versión, auth), spawn
   headless streaming, `resume`, `permission_profiles`, `edit_hooks`,
   `custom_agents`, skills. Requiere el binario `claude` instalado y autenticado en
   el entorno donde se corra — no ejercitable en este sandbox.
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
7. **CI real**: `.github/workflows/ci.yml` no se verificó corriendo en GitHub
   Actions de verdad todavía (solo se replicaron sus pasos localmente antes de cada
   commit). Confirmar en el primer push/PR que dispare el workflow.
8. **`event_hash` (T2.5)**: política ya definida en `docs/eventos.md` §3, sin
   implementar — explícitamente fuera de M-0.
9. **T3.3** — enforcement de presupuesto (conteo de `Usage`, timeout,
   `interrupt → kill`). El mock (T3.2) ya sabe simular una sesión colgada
   justamente para que esta tarea tenga con qué probar el timeout cuando llegue.
10. **T4.1/T4.x (scheduler del DAG)**: no está en el alcance recortado de M-0 que
    veníamos siguiendo, pero es lo que le falta al motor para que `yunta run`
    orqueste un workflow completo (orden por `depends_on`, despacho por `kind` de
    nodo, hooks, re-rutas). Lo que sí existe y ya es reutilizable cuando se
    construya: `run_task` (ciclo de una tarea del ledger) y `dispatch`/dráin de
    eventos del adapter (`task_cycle.rs`, hoy privado a ese módulo).
