---
number: D48
title: "Yunta no mergea a la rama base: crea ramas y PRs; el merge es del CI/branch protection del equipo"
status: accepted
revises: []
revised_by: []
---

# D48 — Yunta no mergea a la rama base: crea ramas y PRs; el merge es del CI/branch protection del equipo

No-responsabilidad explícita y documentada: la concurrencia de merges, los
checks requeridos y los permisos ya los resuelven las merge queues de las
forjas, mejor que cualquier lock propio. Yunta suma: warning de `yunta check`
ante workflows con push directo a la rama base.

Descartado: cola/lock de merges propio (reimplementar merge queues con
mantenimiento eterno para paridad con algo gratis).
