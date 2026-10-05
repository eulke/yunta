---
number: D25
title: "Composición: workflows como nodos (`kind: workflow`), cada sub-workflow un run completo"
status: accepted
revises: []
revised_by: []
---

# D25 — Composición: workflows como nodos (`kind: workflow`), cada sub-workflow un run completo

Runs vinculados (parent/child, promoted_from); artifacts cross-run solo vía
vínculos; padre congela nombres+inputs, no manifests hijos; presupuestos en
cascada; `assignee` en gates para multi-persona.

Descartados: expansión inline, triggers asincrónicos en el engine,
condicionales BPMN.
