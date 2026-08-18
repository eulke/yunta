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
- [ ] **T2.1–T2.3** — storage: SQLite WAL, event log, replay ← **próximo paso**
- [ ] **T3.1** — trait `Adapter`/`AgentSession` + adapter `mock`
- [ ] **T7.3** — adapter `claude-code` real
- [ ] **T5.1–T5.3** — ciclo del ledger: parseo/registro, pre-check en rojo, post-check,
      scope check
- [ ] **T7.1** (parcial) — `yunta run`/`check`/`status`/`resume` de verdad. Hoy: el
      binario `yunta` no tiene subcomandos, solo imprime la versión del engine;
      `check()` existe como función pura en `yunta-engine` (T1.3) pero nada la invoca
      todavía desde el CLI.

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

## Pendiente explícito (para retomar sin adivinar)

1. **T2.1–T2.3** — storage: SQLite en modo WAL, tabla de eventos append-only
   `(run_id, seq, ts, node_id?, kind, payload_json, schema_version)`, interfaz
   mínima (~5 métodos, sin dialectismo SQL fuera del crate), tipos serde de los 31
   `kind` de `docs/eventos.md`, derivación de estado por replay. **Esto sigue ahora.**
2. **T3.1** — trait `Adapter`/`AgentSession` (Spec Adapter v0.2) + adapter `mock`
   completo: fixtures YAML (guión de eventos + efectos de filesystem), fallos y
   latencias inyectables, respeta O1–O6.
3. **T7.3** — adapter `claude-code` real: probe (binario, versión, auth), spawn
   headless streaming, `resume`, `permission_profiles`, `edit_hooks`,
   `custom_agents`, skills.
4. **T5.1–T5.3** — ciclo del ledger completo: parseo y registro (usa
   `docs/spec-ledger.md`), pre-check en rojo, brief mínimo, post-check, scope check
   por `git diff` contra los globs de `scope`.
5. **T7.1 (parcial)** — subcomandos reales `yunta run/check/status/resume` en el
   CLI. `check()` (T1.3) necesita conectarse ahí; falta además cargar el YAML del
   workflow y de la config desde disco (hoy todo se ejercita solo vía tests).
6. **Grupos de config diferidos de T1.2**: `mcp_servers`, `skills`,
   `baseline`/`coverage`, `secrets`, `permissions` (merge invertido, org manda).
   Implementar recién cuando algo los consuma: `baseline`/`coverage` con T5.4
   (`baseline_compare`), `permissions` con T5.7, `skills` con M6, `mcp_servers` con
   M8.
7. **Node kinds diferidos de T1.1**: `gate`, `check`, `parallel`, `executor`,
   `workflow` (composición). Cada uno llega con su milestone correspondiente (M5
   para `check` builtins y gates, M9 para composición) — no antes.
8. **Reglas de `yunta check` diferidas de T1.3**: coherencia de modos, scopes
   disjuntos en `parallel`/`inherit`, templates, profundidad/aciclicidad del grafo
   de workflows, techos de `permissions`, warning de push directo a la rama base.
9. **T1.4** — manifest: resolución y congelado con hashes (workflow+config+
   inputs+modo+runners resueltos+commit base). No nombrada en el alcance mínimo de
   M-0, pero el evento `run_created` (`docs/eventos.md` §5.1) necesita
   `manifest_hash` — en algún punto del ciclo `run`/`resume` hace falta al menos una
   versión recortada (sin `modo`/`inputs`, que no existen todavía).
10. **T1.5** — validación de inputs al crear el run. Depende de `inputs:` en el
    schema, que T1.1 no implementó (fuera del recorte). Diferir hasta que algo
    necesite inputs reales — probablemente cuando el workflow de bootstrap necesite
    parametrizarse.
11. **CI real**: `.github/workflows/ci.yml` no se verificó corriendo en GitHub
    Actions de verdad todavía (solo se replicaron sus pasos localmente antes de cada
    commit). Confirmar en el primer push/PR que dispare el workflow.
