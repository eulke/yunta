---
number: D147
title: "Un adapter real monta el servidor MCP por sesión; la capacidad deja de ser solo del mock"
status: revised
revises: []
revised_by: [D178, D180]
---

# D147 — Un adapter real monta el servidor MCP por sesión; la capacidad deja de ser solo del mock

*(Revisada por D178: la regla de permiso nombra al servidor entero,
`mcp__<servidor>__*`. Revisada por D180: el servidor per-run se llama
`yunta-run`.)* `claude-code` lo recibe como servidor HTTP
externo (`--mcp-config` a un archivo en `run.dir/scratch`, permisos `0600`, y
`--allowedTools mcp__yunta` para permitir el servidor entero sin que el
adapter tenga que conocer la lista de tools); `codex` por overrides `-c`
(`experimental_use_rmcp_client`, `mcp_servers.yunta.url`, y
`bearer_token_env_var` con el token en el entorno del hijo).

Racional: los dos adapters reales declaraban `run_tools: false` con el
comentario «aren't wired yet», así que las cinco herramientas per-run existían
únicamente bajo el mock — es decir, únicamente en los tests. Medido en
sesiones reales: de ocho agentes a los que el bloque montado les pedía llamar
`yunta_check_artifact`, ocho no la encontraron, y dos se escribieron su propio
validador en Rust dentro del repo para conseguir el veredicto por otro lado.
El token viaja por archivo en un caso y por entorno en el otro, nunca en
`argv`, que `ps` le muestra a cualquier proceso local.

Descartados: un subcomando `yunta check-artifact` invocado por Bash
(universal, pero deja muertas las otras cuatro herramientas); inlinear el
token en la config de la línea de comandos (lo publica en la lista de
procesos).
