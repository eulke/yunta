---
number: D63
title: "`isolation: worktree | none`; `container` eliminado del schema (Contrato §7.3)"
status: accepted
revises: []
revised_by: []
---

# D63 — `isolation: worktree | none`; `container` eliminado del schema (Contrato §7.3)

`worktree` default (concurrencia y aislamiento del usuario). `none` para tres
casos legítimos —编 editor en vivo, CI ya contenido, setup de árbol
prohibitivo— con condiciones duras: árbol limpio exigido por el engine (sin
eso el scope por diff no distingue agente de usuario), sin runs concurrentes
en ese repo, y modo registrado en manifest y recibo (menos garantías, nunca
ocultas). `container` estaba declarado pero no diseñado: prometer aislamiento
por contenedor sin especificarlo es la "seguridad aparente" que §6.1 rechaza —
dos valores reales mejor que tres con uno hueco. Puede volver como extensión
cuando exista diseño.
