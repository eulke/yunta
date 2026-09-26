---
number: D106
title: "Materialización de contenido efectivo como condición de replayability (Contrato §9)"
status: revised
revises: []
revised_by: [D157]
---

# D106 — Materialización de contenido efectivo como condición de replayability (Contrato §9)

*(Revisada por D157: el contenido efectivo vive en el mismo store que los
artifacts, `objects/<hash>`; `context/` no existe.)* El hash de una fuente de
contexto identifica y deduplica; el contenido bajo `context/<hash>/` es lo que
hace posible reconstruir qué vio un agente sin volver a consultar el origen.
Regla explícita por fuente (files: snapshot; command: stdout/stderr+exit;
knowledge: contenido resuelto; mcp: request+response;
run-events/ledger/node-output: el fragmento leído). No es mecanismo nuevo — ya
estaba implícito en la firma de `ResolvedContext` — pero se nombra para que
ninguna fuente builtin lo incumpla por comodidad, y para que `replay`
(post-v1) tenga una garantía real que cumplir.
