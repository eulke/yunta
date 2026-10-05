---
number: D99
title: "Idempotencia obligatoria bajo `restart_node`; tercer valor `fail_if_uncertain` para el residual (Contrato §8.1)"
status: accepted
revises: []
revised_by: []
---

# D99 — Idempotencia obligatoria bajo `restart_node`; tercer valor `fail_if_uncertain` para el residual (Contrato §8.1)

Corrección de una conflación en el propio diseño: I8 (rehidratación) garantiza
que un nodo puede *arrancar* de cero con contexto correcto, no que
reejecutarlo sea inocuo — D15 había usado I8 para justificar que
`restart_node` "siempre es seguro", conflando dos propiedades distintas. Un
nodo con efectos externos (`git push` + `gh pr create`, un webhook, un sistema
de terceros) puede duplicar o corromper si se reintenta a ciegas tras un crash
a mitad. Solución: extender la exigencia de idempotencia de hooks (I13) a todo
nodo bajo `restart_node` — patrón check-then-act preferido sobre comandos que
fallan sin duplicar; y agregar `fail_if_uncertain` para el residual
genuinamente no idempotente (notificaciones sin dedup key, cobros): si al
reanudar el engine encuentra un nodo `running` sin evento terminal, pausa con
gate en vez de asumir.

Descartado: dejar `restart_node` como único modo confiando en que los autores
de workflow noten el riesgo por su cuenta (silencioso, exactamente lo que I11
prohíbe).
