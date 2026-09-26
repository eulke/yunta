---
number: D60
title: "Agilidad por diseño del workflow, no por apagar la verificación"
status: accepted
revises: []
revised_by: []
---

# D60 — Agilidad por diseño del workflow, no por apagar la verificación

Quien necesita velocidad escribe un workflow chico (sin ledger, los criterios
obligatorios aplican a tareas del ledger, no a nodos) o declara un modo que
salta deliberación — ambos ya existen. No habrá kill switch de verificación:
un run que puede terminar sin verificar nada vacía de significado al recibo
(D54) y con él al diferencial entero del producto. Si el costo temporal del
pre-check llegara a doler pese a D59, la única palanca admisible es un dial
acotado (saltear pre-check, conservar post-check) declarado en el recibo y en
stats, y acotable desde la capa org vía `permissions` — nunca silencioso,
nunca total. Estado: no se implementa hoy; D59 elimina su razón de ser
previsible.
