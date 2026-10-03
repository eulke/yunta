---
number: D10
title: "Manifest inmutable congelado al crear el run"
status: accepted
revises: []
revised_by: []
---

# D10 — Manifest inmutable congelado al crear el run

El engine nunca relee workflows durante un run; cambiar reglas o modo = run
sucesor. Elimina bugs de reproducibilidad por edición concurrente.
