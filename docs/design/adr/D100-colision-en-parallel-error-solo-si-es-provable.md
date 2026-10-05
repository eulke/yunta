---
number: D100
title: "Colisión en `parallel`: error solo si es provable, warning si no se puede saber (Contrato §5.8)"
status: accepted
revises: []
revised_by: []
---

# D100 — Colisión en `parallel`: error solo si es provable, warning si no se puede saber (Contrato §5.8)

Versión corregida de una propuesta inicial que asumía que el engine podía
comparar diffs de hermanos post-hoc para detectar colisiones automáticamente —
errónea: sin worktree por hijo (deliberado, para no cargar `parallel` con el
peso de `concurrency`) y sin capacidad de tracking en nodos `bash`, no existe
forma de atribuir qué archivo tocó cada hijo cuando corren de verdad en
simultáneo. Diseño final: scopes declarados y solapados → error en `check`
(estático, sí verificable); sin scope declarado en hijos que pueden escribir →
warning recomendando declararlo, nunca una afirmación de detección que el
sistema no puede cumplir.

Descartado: worktree por hijo estático (rompe la liviandad de D97), y
detección automática por diff (implementable solo en apariencia — exactamente
la seguridad aparente que §6.1 ya rechaza en otro contexto).
