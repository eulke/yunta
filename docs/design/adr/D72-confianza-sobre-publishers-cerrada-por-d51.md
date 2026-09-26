---
number: D72
title: "Confianza sobre publishers: cerrada por D51, sin granularidad adicional"
status: accepted
revises: []
revised_by: []
---

# D72 — Confianza sobre publishers: cerrada por D51, sin granularidad adicional

`permissions.packs` con `publishers.allow` y `executors: allow|prompt|deny`
cubre la gobernanza completa (qué publishers se aceptan y si su código
ejecutable corre), como techo org que las capas inferiores solo estrechan.
Descartada una matriz de confianza por publisher (p. ej. "de Acme acepto
executors, de otros solo contenido declarativo"): hoy se resuelve combinando
la allowlist con `executors: prompt`, que pide confirmación caso por caso; una
matriz es complejidad especulativa sin caso real que la pida.
