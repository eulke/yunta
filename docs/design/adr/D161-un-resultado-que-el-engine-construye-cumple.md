---
number: D161
title: "Un resultado que el engine construye cumple la revisión más nueva que el servidor anuncia; un cliente anterior ignora lo que no entiende"
status: accepted
revises: []
revised_by: []
---

# D161 — Un resultado que el engine construye cumple la revisión más nueva que el servidor anuncia; un cliente anterior ignora lo que no entiende

Los dos servidores MCP de yunta —el listener por sesión del engine sobre HTTP
loopback (D147–D150) y el plano de control de `yunta mcp` sobre stdio—
anuncian toda la lista de revisiones que el SDK implementa, hoy `2024-11-05` …
`2026-07-28`, y contestan las dos eras con los mismos handlers. Entonces todo
resultado que construyen es válido en la más nueva: un resultado de lista
lleva `resultType`, `ttlMs` y `cacheScope`, y la decisión de qué lleva vive en
un solo lugar (`yunta_engine::mcp::tool_list`) que consumen los dos
servidores. `ttlMs: 0` no es un relleno: la lista de tools se arma por sesión
y por nodo, así que está vieja apenas se lee; `private` porque nombra lo que
puede hacer un solo llamador autenticado.

Racional: `2026-07-28` (SEP-2549) volvió obligatorios los dos campos de cache
en todo resultado de lista, y `rmcp` los modela `Option` con
`skip_serializing_if`, así que un handler escrito a mano no los emite. Medido
contra `claude-code` 2.1.270: la sesión negociaba `2026-07-28`, el
`server/discover` pasaba —ese resultado sí los lleva, porque `DiscoverResult`
los tiene obligatorios en el tipo— y el `tools/list` se caía con
`[{"path":["ttlMs"],"code":"invalid_type"},{"path":["cacheScope"],"code":"invalid_value"}]`,
tres reintentos y `Failed to fetch tools`; el agente abría sin ninguna tool
`mcp__yunta__*` y el nodo cerraba con `undelivered` debiendo el documento que
declaraba. El servidor estaba bien por lo demás (protocolo negociado entero,
`tools/list` contestado en 2 ms), y CI no lo veía porque sus tests usan el
cliente de `rmcp`, cuyo `ProtocolVersion::LATEST` es `2025-11-25`: negocian
por `initialize`, la era donde los campos de cache no se validan. La otra
mitad de la regla la garantiza el propio SEP-2549: *«Existing clients that do
not understand the field will ignore it»*. Por eso cada era tiene ahora su
test sobre el JSON crudo, sin cliente de `rmcp` en el medio.

Descartados: **forzar la era legacy** acotando `supported_protocol_versions`
(prioriza lo viejo y depende de que el cliente acepte retroceder, además de
renunciar a todo lo que la revisión nueva trae); **servir las run tools por
stdio** en vez de HTTP (funcionaría solo porque `claude-code` hoy negocia
stdio como legacy, el token dejaría de viajar como cabecera, y las razones de
D147–D150 para el loopback HTTP siguen vigentes); **esperar a que `rmcp` los
emita** (3.3.0 tampoco lo hace en un handler manual —la macro de resultados
paginados y su `Default` son los mismos— y SEP-2549 dice explícitamente que
rellenarlos es opcional para un SDK); **rellenarlos solo en el servidor por
sesión** y no en `yunta mcp` (el de la CLI tiene el mismo defecto, invisible
únicamente porque `claude-code` negocia stdio como legacy, y una regla que
vale para un servidor y no para el otro es la segunda copia que «Un lugar»
prohíbe).
