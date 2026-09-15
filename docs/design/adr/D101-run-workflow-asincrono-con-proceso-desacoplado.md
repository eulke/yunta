---
number: D101
title: "`run_workflow` asíncrono con proceso desacoplado de la sesión MCP (Contrato §6.4)"
status: accepted
revises: []
revised_by: []
---

# D101 — `run_workflow` asíncrono con proceso desacoplado de la sesión MCP (Contrato §6.4)

Retorna `run_id` de inmediato; internamente dispara `yunta run --detach`, un
proceso independiente de `yunta mcp` que sigue vivo si el cliente cierra.
Alternativa descartada — ejecutar el run como tarea asíncrona dentro del
propio proceso `yunta mcp` — habría hecho que la vida del run dependiera de la
sesión MCP, convirtiendo a `yunta mcp` en un daemon de facto mientras el run
dura, contra D05/§7. Seguimiento por poll (`workflow_status`), nunca push —
mismo modelo pull que gates externos (D66), sin webhooks ni notificaciones del
lado del engine.
