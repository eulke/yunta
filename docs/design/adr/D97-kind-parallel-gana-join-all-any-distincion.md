---
number: D97
title: "`kind: parallel` gana `join: all|any`; distinción explícita frente a `concurrency` (Contrato §5.8)"
status: accepted
revises: []
revised_by: []
---

# D97 — `kind: parallel` gana `join: all|any`; distinción explícita frente a `concurrency` (Contrato §5.8)

`parallel` es para nodos estáticos conocidos de antemano por el autor;
`concurrency` (D65) es para tareas de un ledger cuyo número no existe hasta
que el plan corre. `join: all` (default) formaliza el comportamiento implícito
(falla uno, falla el grupo); `join: any` completa con el primer éxito e
interrumpe al resto (mismo mecanismo de cancelación ordenada de la Spec
Adapter). Cierra un gap real: el Contrato nunca especificó cuándo termina un
grupo `parallel`.
