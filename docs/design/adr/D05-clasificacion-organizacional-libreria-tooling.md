---
number: D05
title: "Clasificación organizacional: librería/tooling sin infraestructura"
status: revised
revises: []
revised_by: [D77]
---

# D05 — Clasificación organizacional: librería/tooling sin infraestructura

*(Revisada por D77: sin excepciones — `yunta serve` salió de Yunta por
completo, y no hay feature flag ni compilación condicional que lo prevea.)*
Corre en la máquina del dev o en CI y termina; sin servidores, instancias ni
deploys. `yunta mcp` NO es un daemon: el cliente MCP lanza el proceso por
stdio y lo mata al cerrar.
