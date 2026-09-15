---
number: D12
title: "Criterios ejecutables test-first con pre-check en rojo"
status: accepted
revises: []
revised_by: []
---

# D12 — Criterios ejecutables test-first con pre-check en rojo

Todo criterio no-`guard` debe fallar antes del trabajo (si ya pasa, es trivial
→ rebota a plan) y pasar después. `guard` = no-regresión local (pasa antes y
después). "Criterio inútil" pasa de juicio a exit code.
