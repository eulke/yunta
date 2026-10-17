---
number: D96
title: "Exportación de telemetría OTel: trace por run, span por nodo, proyectados por replay (Contrato §8.8)"
status: accepted
revises: []
revised_by: []
---

# D96 — Exportación de telemetría OTel: trace por run, span por nodo, proyectados por replay (Contrato §8.8)

Inerte hasta T13.3. Trace con atributos de identidad y resultado del run
(`yunta.run_id/workflow/mode/manifest_hash/outcome/tokens_total/cptv`); span
por nodo proyectado uno a uno desde el event log
(`yunta.node_id/node_kind/runner_role/runner_resolved/outcome/retries/tokens_*`).
Generados por replay, nunca una fuente de verdad paralela — un run viejo se
puede exportar retroactivamente y jamás hay dos historias del mismo run.
Prefijo `yunta.` sin adoptar aún las convenciones `gen_ai.*` de OTel (en
movimiento; migrar será un mapeo de nombres, no un rediseño). **Los spans
heredan las reglas de redacción de los eventos (I12)**: metadata de dónde se
fue el tiempo y el costo, nunca contenido — ni prompt, ni output, ni paths con
datos, ni siquiera resumidos.

Descartado: adoptar `gen_ai.*` ya (estándar inmaduro) y exportar spans con
contenido de agente (violaría I12 por otra puerta).
