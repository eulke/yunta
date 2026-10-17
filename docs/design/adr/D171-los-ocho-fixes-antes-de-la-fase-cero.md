---
number: D171
title: "Ocho correcciones de comportamiento van antes de la fase 0, cada una como el mecanismo aplicado a un solo sitio"
status: accepted
revises: []
revised_by: []
---

# D171 — Ocho correcciones de comportamiento van antes de la fase 0, cada una como el mecanismo aplicado a un solo sitio

W-01 a W-08 de `docs/design/plan-de-raiz/README.md` §4 se implementan antes
de la fase 0 del plan, en este orden, cada una un PR con su test en rojo
primero: sesiones de `loop` en el modelo resuelto; nombre de artifact
validado después de renderizar; `target_digest` siempre hash; blackboard por
`FindingLedger` y una regla de dedup; git por `spawn_governed`;
`parallel_exec` por `resume_policies`; `MockSession` con handle y `Drop`;
`run_yunta` y `Terminal::open` herméticos. La regla que las habilita: cada
corrección es un subconjunto estricto de su mecanismo — código que la fase
igual escribiría, en el mismo lugar — y nunca una copia más.

Racional: un usuario que corre Yunta hoy sufre los ocho (modelo equivocado,
escape del run dir, secretos en el log, findings retirados visibles, git
colgado que sobrevive a Ctrl-C, `on_interrupt` ignorado en grupos, tasks
filtradas, tests que leen el host); esperar tres fases no es aceptable, y los
ocho tienen un fix que no se tira.

Descartados: hotfixes que no sean subconjunto del mecanismo (por ejemplo un
quinto gate inline para las capacidades no consultadas — eso espera M09);
esperar a la fase correspondiente.
