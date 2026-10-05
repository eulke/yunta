---
number: D36
title: "`skills:` y `context:` son propiedades separadas del nodo"
status: accepted
revises: []
revised_by: []
---

# D36 — `skills:` y `context:` son propiedades separadas del nodo

Contexto = datos (qué), materializados por el engine vía `ContextSource`;
skills = instrucciones/capacidades (cómo), montadas por el mecanismo nativo
del adapter.

Descartado: modelar skills como una fuente de contexto más — confundiría datos
con comportamiento y acoplaría el montaje de skills al pipeline de contexto.
