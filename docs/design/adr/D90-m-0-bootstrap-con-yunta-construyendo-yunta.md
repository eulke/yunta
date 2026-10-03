---
number: D90
title: "M-0: bootstrap con Yunta construyendo Yunta, antes de completar M0–M7"
status: accepted
revises: []
revised_by: []
---

# D90 — M-0: bootstrap con Yunta construyendo Yunta, antes de completar M0–M7

Corte transversal mínimo (specs de ledger y eventos, workspace, storage,
schema recortado con re-rutas, trait Adapter con `mock` y `claude-code`, ciclo
del ledger completo, `run`/`check`/`status`/`resume`) cuyo objetivo no es
entregar producto sino validar la tesis del engine contra la realidad y ganar
confianza productiva para ejecutar el resto del plan con el propio sistema.
**Ledgers escritos a mano y just-in-time** durante el bootstrap: elimina la
dependencia de que un agente produzca ledgers válidos justo cuando se valida
el ciclo de verificación (dos incertidumbres a la vez es una de más), y evita
reescribir ochenta ledgers contra un schema que se va a ajustar — los
criterios buenos salen del contexto real. Criterio de éxito: la primera tarea
del plan implementada por Yunta sobre sí mismo con criterios verdes y scope
limpio, elegida entre M6–M7 (autocontenidas), nunca de M0–M2 (el andamio). Las
tres preguntas que debe responder: practicabilidad del pre-check en rojo,
ruido del scope por diff, y confiabilidad de los ledgers generados por
agentes. Riesgo reconocido y documentado: el bootstrap sesga el diseño hacia
Rust y un solo usuario — packs, composición multi-persona y flujos no-software
necesitan validación aparte.
