---
number: D79
title: "`kind: findings` como segundo artifact interpretado (Contrato §4.1)"
status: accepted
revises: []
revised_by: []
---

# D79 — `kind: findings` como segundo artifact interpretado (Contrato §4.1)

Los hallazgos dejan de ser prosa opaca y pasan a ser datos del run: cada
entrada declara `id`, `severity` (blocking/major/minor/note), `title`,
`location` y `detail`, más `proposed_criterion` opcional; el engine parsea el
archivo al cerrar el nodo y emite un `finding_posted` por entrada. Gana:
conteo y agrupación por severidad, deduplicación entre reviewers por
`location` + título normalizado, presencia en `status` y en el recibo, y
supervivencia como datos para el gate de promoción o la destilación. El nodo
consolidador sigue existiendo para el juicio (qué corregir, qué diferir), pero
recibe datos en lugar de tres documentos con estructuras distintas. Se
formaliza además la distinción general: artifact **opaco** por default (el
engine verifica existencia y hash, la estructura interna es del agente) vs
**interpretado** vía `kind` (hay parser, schema y validación; un archivo mal
formado falla el nodo). La vara para agregar un `kind` es alta: solo cuando el
engine necesita los datos para decidir o contar. `knowledge` permanece opaco
deliberadamente — su valor es ser prosa legible por personas y agentes, y
estructurarlo lo empobrecería sin dar al engine nada que necesite.
