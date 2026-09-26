---
number: D26
title: "Comunicación entre nodos mediada por el engine, jamás directa"
status: accepted
revises: []
revised_by: []
---

# D26 — Comunicación entre nodos mediada por el engine, jamás directa

Canal default: artifacts (durable, auditado, en el DAG). Lectura del log vía
`run-events`. Para paralelos: blackboard append-only mediado por el engine vía
MCP por-run (`yunta_post_finding`) — opción por workflow, en tensión
documentada con independencia total de reviewers (ver Deuda).

Descartado: mensajería dirigida agente-a-agente (no determinista).
