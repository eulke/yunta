---
number: D05
title: "Clasificación organizacional: librería/tooling sin infraestructura"
status: revised
revises: []
revised_by: []
---

# D05 — Clasificación organizacional: librería/tooling sin infraestructura

*(Revisada: sin excepciones — D77 sacó **`yunta serve`** de Yunta por
completo; ya no hay feature flag ni compilación condicional que lo prevea.)*
Corre en la máquina del dev o en CI y termina; sin servidores, instancias ni
deploys. `yunta mcp` NO es un daemon: el cliente MCP lanza el proceso por
stdio y lo mata al cerrar.
