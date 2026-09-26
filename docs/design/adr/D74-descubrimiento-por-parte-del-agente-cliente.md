---
number: D74
title: "Descubrimiento por parte del agente cliente: skill de mecanismo + catálogo consultado, nunca escrito"
status: accepted
revises: []
revised_by: []
---

# D74 — Descubrimiento por parte del agente cliente: skill de mecanismo + catálogo consultado, nunca escrito

Un agente que ve `run_workflow` no infiere solo que conviene usarlo, y menos
aún qué workflows existen en ese repo. Defensa en capas: (a) **descripciones
de tools escritas para decidir**, no para describir — explicitan cuándo
preferir un workflow verificado sobre implementar directo; (b) **skill
instalada por `init`** que enseña el mecanismo y cómo consultar el catálogo
(`yunta list` / `list_workflows`) — es estable, no se regenera y no caduca;
(c) **línea sugerida para el CLAUDE.md del repo**, ofrecida en modo
interactivo e impresa para copiar en modo no interactivo — **jamás escrita
automáticamente**: ese archivo es del equipo, y una herramienta que se
autoinserta ahí genera la misma desconfianza que los hooks invisibles (D23).
Decisión estructural: **el catálogo se consulta en el momento vía
`list_workflows`, nunca se escribe dentro de la skill** — `init` corre cuando
aún no hay workflows, y un catálogo embebido quedaría vacío al nacer y
desactualizado apenas alguien cree uno. Honestidad de producto documentada:
estos mecanismos inclinan el comportamiento, no lo garantizan; la única
garantía dura es que el repositorio exija el recibo (D54) como check requerido
del PR — sin run verificado, no hay merge. Eso es decisión del equipo, no algo
que Yunta imponga.
