---
number: D104
title: "Reproducibilidad histórica de child runs explícita, sin duplicar manifest (Contrato §12)"
status: accepted
revises: []
revised_by: []
---

# D104 — Reproducibilidad histórica de child runs explícita, sin duplicar manifest (Contrato §12)

`child_run_created` suma `workflow_hash` del hijo a su payload; la regla — ya
implícita en I3+I15 pero nunca dicha en una frase — se hace explícita: el
`child_run_id` es la referencia inmutable, el manifest congelado del hijo (que
I3 ya garantiza para todo run) es su propia fuente histórica de verdad, y
reproducir un padre jamás re-resuelve `workflow@current` para sus hijos.

Descartado: duplicar el manifest completo del hijo dentro del padre
(redundante — el hijo ya es inmutable por sí mismo, I3 alcanza).
