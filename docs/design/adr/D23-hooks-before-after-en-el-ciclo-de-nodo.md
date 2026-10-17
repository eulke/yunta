---
number: D23
title: "Hooks `before`/`after` en el ciclo de nodo del ENGINE"
status: accepted
revises: []
revised_by: []
---

# D23 — Hooks `before`/`after` en el ciclo de nodo del ENGINE

No del adapter: determinísticos, idempotentes, bajo scope, con evento. Orden:
contexto → before → sesión → after → verificación.

Descartado: hooks en el adapter (cada adapter los reimplementaría) y hooks
inyectados desde capas invisibles al workflow.
