---
number: D52
title: "Layout de tres ámbitos con potestad explícita; paths de estado configurables (Contrato §2.2)"
status: accepted
revises: []
revised_by: []
---

# D52 — Layout de tres ámbitos con potestad explícita; paths de estado configurables (Contrato §2.2)

Proyecto (`.yunta/`, versionado, del equipo): config, workflows, skills,
knowledge, packs. Usuario (`~/.yunta/`): config personal y TODO el estado de
ejecución (runs, worktrees, DB). Org (`/etc/yunta/`): defaults y techo de
permisos. Estado jamás en el repo: los runs son de quien los corre y no pueden
vivir en el árbol de git que los agentes manipulan. Potestad del usuario sobre
ubicación del estado: `paths.{runs,worktrees}` en config + `YUNTA_HOME` como
override de raíz (CI efímero, disco, políticas de datos); los paths resueltos
se congelan en el manifest — un resume nunca busca el run.dir donde la config
actual diga, sino donde el run nació.

Descartado: estado dentro del repo (contaminación del árbol + colisión
multiusuario) y paths hardcodeados sin override (muerde en CI y entornos
restringidos).
