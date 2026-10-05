---
number: D150
title: "`read-only` se audita contra nada, y por eso puede escribir lo que declara"
status: accepted
revises: []
revised_by: []
---

# D150 — `read-only` se audita contra nada, y por eso puede escribir lo que declara

`audited_scope` devuelve el scope declarado, o el vacío cuando el nodo es
`read-only`; el cierre audita el diff del worktree contra eso con el mismo
evento `scope_checked` de siempre. El perfil `ReadOnly` pasa a incluir
`Write`, porque lo que un perfil dice es qué le puede hacer la sesión *al
proyecto*, y el artifact declarado de un nodo no es el proyecto: es su salida,
en el directorio del run, que `--add-dir` es lo que abre.

Racional: `check/scopes.rs` ya exime a un nodo `read-only` de toda regla de
scope solapado, dejándolo correr al lado de cualquier otro; esa exención se
apoyaba en la palabra y nada la hacía cierta, porque un nodo sin `scope:`
declarado no se auditaba. Sin `Write`, además, un nodo read-only que declaraba
un artifact no podía producirlo — el `plan` del pack de referencia es
exactamente ese caso.

Descartados: confinar la escritura por flags del CLI (probado vivo: bajo
`acceptEdits` la regla `Edit(...)` de denegación no bloquea); rechazar la
combinación read-only + produces en `yunta check` (deja el caso legítimo sin
forma de expresarse).
