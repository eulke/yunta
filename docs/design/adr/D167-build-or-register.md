---
number: D167
title: "Lo prometido y no construido se construye o se registra; nunca queda como comentario"
status: accepted
revises: [D18, D62, D19]
revised_by: []
---

# D167 — Lo prometido y no construido se construye o se registra; nunca queda como comentario

Cinco comportamientos que la documentación promete y el código no tiene se
resuelven así:

- **Baseline al crear el run** (D18, Contrato §7.2): **se construye.**
  `create_run` captura la suite declarada en `baseline.suite` después del
  worktree y persiste resultados y hash bajo `baseline/`; el primer
  `baseline_compare` compara contra eso. La razón del atajo actual
  ("haría `create_run` async en sus cuatro llamadores") no vale: `create_run`
  ya es async en su camino principal.
- **Orden de criterios aprendido del log** (D62, Contrato §5.4): **se
  construye.** `criteria_checked.results[].duration_ms` ya se persiste; la
  memoización lee ese historial desde `TaskLedger` en vez de un `Mutex` por
  invocación. El Contrato §5.4 se corrige para decir que la cache de
  resultados es por invocación (Replay) y el orden es del log.
- **Hooks de edición** (spec-adapter §6): **se registra** como deuda `A-13`.
  Ningún adapter los tiene; el engine degrada con
  `capability_degraded(PostCheckOnly)` una vez por run, y spec-adapter §6 dice
  la verdad sobre cada adapter.
- **Preguntas por pull request** (Contrato §3, §4.1): **se registra** como
  deuda `A-14`. `Channel` queda `{Tty, Mcp}`; el Contrato se corrige.
- **Fuentes de contexto por executor** (D19, Contrato §9): **se registra**
  como deuda `A-15`. `ContextSpec` queda cerrada; D19 gana nota de revisión.

Los comentarios que hoy explican cada atajo (`check_exec.rs:77-82`,
`criteria.rs:29-36`, `session.rs:3-10`) se borran.

Racional: CLAUDE.md — la documentación gana al código salvo decisión
registrada; un comentario que explica un atajo es una decisión que alguien
tomó solo.

Descartados: construir los cinco (hooks de edición y preguntas por PR
dependen de capacidades que ningún adapter ni forja declara hoy; fuentes por
executor es un extension point sin diseño); registrar los cinco (baseline y
orden desde el log son cambios chicos que cierran garantías centrales del
Contrato).
