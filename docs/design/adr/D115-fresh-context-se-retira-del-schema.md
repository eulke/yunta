---
number: D115
title: "`fresh_context` se retira del schema"
status: accepted
revises: []
revised_by: []
---

# D115 — `fresh_context` se retira del schema

El comportamiento que describía — cada tarea arranca en una sesión recién
nacida — es I8 y está garantizado sin opt-in; la única variante real es
`on_interrupt: resume_session`, que reutiliza una sesión al reanudar. `check`
rechaza la clave como desconocida (D110) con un mensaje que nombra
`on_interrupt`.

Racional: dos claves para un concepto son una fuente de ambigüedad, y una
clave que el engine ignora es una promesa vacía.

Descartado: implementarla como sinónimo de `on_interrupt: restart_node`
(duplica una clave existente).
