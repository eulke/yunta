---
number: D28
title: "Keyword `runner:`, no `role:`"
status: accepted
revises: []
revised_by: []
---

# D28 — Keyword `runner:`, no `role:`

Razón: el campo responde "qué ejecuta esto" (precedente `runs-on` de GH
Actions); `role:` colisionaría con `assignee` humanos y con "role"
sobrecargado en el ecosistema (user/assistant, RBAC); un solo keyword cubre
nombre-de-rol y referencia concreta. "Rol" queda como vocabulario descriptivo
de la spec.
